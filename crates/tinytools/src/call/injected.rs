//! Model-call identity and host-injected argument preparation.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Stable identity of one model-requested tool call.
///
/// The identity belongs to an invocation, never a [`crate::ToolResult`]: a
/// result reports tool-owned content, while a host correlates that content to
/// its request, timing, and event stream separately.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolCallId(String);

impl ToolCallId {
    /// Creates a call identity from a host- or provider-assigned value.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the provider- or host-assigned identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ToolCallId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A parsed model request to invoke a named tool.
///
/// `arguments` contains only model-supplied values. A host must prepare it
/// with [`prepare_tool_arguments`] before schema validation and execution when
/// the selected tool declares injected arguments.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// The host- or provider-assigned identity for this invocation.
    pub id: ToolCallId,
    /// Canonical name of the requested tool.
    pub name: String,
    /// Arguments parsed from the model response.
    pub arguments: Value,
}

impl ToolCall {
    /// Creates a model-requested tool call.
    #[must_use]
    pub fn new(id: ToolCallId, name: impl Into<String>, arguments: Value) -> Self {
        Self {
            id,
            name: name.into(),
            arguments,
        }
    }
}

/// Authoritative source of an injected tool argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolInjectedArgumentSource {
    /// A value supplied explicitly by the host for this invocation.
    Host,
    /// The [`ToolCall::id`] as a JSON string.
    ToolCallId,
}

/// An argument a tool receives from an authoritative source, never a model.
///
/// A tool declares these through [`crate::Tool::injected_arguments`]. Hosts
/// must remove model-supplied values for every declared name, inject the
/// authoritative values, and only then validate the resulting object against
/// the tool schema.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolInjectedArgument {
    /// Name of the argument in the tool's JSON Schema.
    pub name: String,
    /// Source that owns the argument's value.
    pub source: ToolInjectedArgumentSource,
}

impl ToolInjectedArgument {
    /// Declares an argument that the host supplies per invocation.
    #[must_use]
    pub fn host(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: ToolInjectedArgumentSource::Host,
        }
    }

    /// Declares an argument populated from the canonical tool-call identity.
    #[must_use]
    pub fn tool_call_id(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: ToolInjectedArgumentSource::ToolCallId,
        }
    }
}

/// Runtime-only values that a host supplies for declared injected arguments.
///
/// This type intentionally has no serde implementation. It can hold sensitive
/// values, and host injection values are execution inputs rather than
/// model-visible or durable tool-call data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InjectedToolArguments {
    values: BTreeMap<String, Value>,
}

impl InjectedToolArguments {
    /// Creates an empty host-value collection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces one host-owned argument value.
    pub fn insert(&mut self, name: impl Into<String>, value: Value) -> Option<Value> {
        self.values.insert(name.into(), value)
    }

    /// Returns a host-owned value by declaration name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
    }
}

/// Failure while preparing model arguments for schema validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolArgumentPreparationError {
    /// The model supplied a non-object value where an argument object is
    /// required for stripping and host injection.
    ArgumentsMustBeObject,
    /// The tool declared the same argument name from multiple sources.
    DuplicateDeclaration {
        /// Duplicate argument name.
        name: String,
    },
    /// A host-owned declaration had no corresponding host value.
    MissingHostValue {
        /// Name of the missing host-owned argument.
        name: String,
    },
}

impl fmt::Display for ToolArgumentPreparationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ArgumentsMustBeObject => {
                formatter.write_str("tool arguments must be a JSON object")
            }
            Self::DuplicateDeclaration { name } => {
                write!(formatter, "duplicate injected argument declaration: {name}")
            }
            Self::MissingHostValue { name } => {
                write!(
                    formatter,
                    "missing host value for injected argument: {name}"
                )
            }
        }
    }
}

impl std::error::Error for ToolArgumentPreparationError {}

/// Removes declared injected keys from a schema's `properties` and `required`
/// members for the model-facing projection.
///
/// The tool's declared schema remains the introspection and validation shape.
/// A host sends this projection to the model so it never requests a value that
/// only the host or the canonical call identity may supply. Schemas without an
/// object `properties` or array `required` member are returned unchanged.
#[must_use]
pub fn project_injected_arguments(schema: &Value, declarations: &[ToolInjectedArgument]) -> Value {
    let mut projected = schema.clone();
    let Some(object) = projected.as_object_mut() else {
        return projected;
    };

    if let Some(Value::Object(properties)) = object.get_mut("properties") {
        for declaration in declarations {
            properties.remove(&declaration.name);
        }
    }

    if let Some(Value::Array(required)) = object.get_mut("required") {
        required.retain(|value| {
            value.as_str().is_none_or(|name| {
                !declarations
                    .iter()
                    .any(|declaration| declaration.name == name)
            })
        });
    }

    projected
}

/// Strips model-supplied injected values, applies authoritative values, and
/// returns the schema-ready argument object.
///
/// A host calls this before schema validation. It must validate the returned
/// value rather than the model-supplied [`ToolCall::arguments`], otherwise a
/// model can forge an injected value or a required injected field can appear
/// absent to the validator.
///
/// # Errors
///
/// Returns [`ToolArgumentPreparationError`] when the model arguments are not
/// an object, an injected name is declared more than once, or a host-owned
/// declaration has no host value.
pub fn prepare_tool_arguments(
    call: &ToolCall,
    declarations: &[ToolInjectedArgument],
    host_values: &InjectedToolArguments,
) -> Result<Value, ToolArgumentPreparationError> {
    let Some(arguments) = call.arguments.as_object() else {
        return Err(ToolArgumentPreparationError::ArgumentsMustBeObject);
    };
    let mut prepared = arguments.clone();

    for declaration in declarations {
        if declarations
            .iter()
            .filter(|other| other.name == declaration.name)
            .nth(1)
            .is_some()
        {
            return Err(ToolArgumentPreparationError::DuplicateDeclaration {
                name: declaration.name.clone(),
            });
        }

        // Remove model input before retrieving the authoritative source. This
        // order is security-relevant: host injection must replace, never merge
        // with, a model-provided value for a protected key.
        prepared.remove(&declaration.name);
        let value = match declaration.source {
            ToolInjectedArgumentSource::Host => host_values
                .get(&declaration.name)
                .cloned()
                .ok_or_else(|| ToolArgumentPreparationError::MissingHostValue {
                    name: declaration.name.clone(),
                })?,
            ToolInjectedArgumentSource::ToolCallId => Value::String(call.id.to_string()),
        };
        prepared.insert(declaration.name.clone(), value);
    }

    Ok(Value::Object(prepared))
}
