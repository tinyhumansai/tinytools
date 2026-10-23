//! Compile-time source for entries created by the built-in catalog.

pub(super) fn source(id: &str) -> Option<&'static str> {
    match id {
        "rust.fallible-function.v1" => {
            Some(include_str!("templates/rust.fallible-function.v1.txt"))
        }
        "rust.tauri-command.v1" => Some(include_str!("templates/rust.tauri-command.v1.txt")),
        "typescript.typed-function.v1" => {
            Some(include_str!("templates/typescript.typed-function.v1.txt"))
        }
        "tsx.accessible-action.v1" => Some(include_str!("templates/tsx.accessible-action.v1.txt")),
        "python.resource-context.v1" => {
            Some(include_str!("templates/python.resource-context.v1.txt"))
        }
        "go.http-handler.v1" => Some(include_str!("templates/go.http-handler.v1.txt")),
        "typescript.vitest-suite.v1" => {
            Some(include_str!("templates/typescript.vitest-suite.v1.txt"))
        }
        "json.service-config.v1" => Some(include_str!("templates/json.service-config.v1.txt")),
        "toml.service-config.v1" => Some(include_str!("templates/toml.service-config.v1.txt")),
        "rust.checked-add.v1" => Some(include_str!("templates/rust.checked-add.v1.txt")),
        "typescript.nullable-guard.v1" => {
            Some(include_str!("templates/typescript.nullable-guard.v1.txt"))
        }
        "rust.public-field.v1" => Some(include_str!("templates/rust.public-field.v1.txt")),
        _ => None,
    }
}
