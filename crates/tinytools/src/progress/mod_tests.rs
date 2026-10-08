//! Unit tests for `ToolProgress` and `ProgressSink`: the builders, the pinned
//! wire form, and that a sink delivers to its closure (and a no-op sink does
//! nothing).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use serde_json::json;

use super::*;
use crate::ToolRunContext;

#[test]
fn a_bare_update_carries_nothing() {
    let update = ToolProgress::default();
    assert!(update.is_empty());
    assert_eq!(update.message, None);
    assert_eq!(update.fraction, None);
    assert_eq!(update.partial, None);
}

#[test]
fn builders_set_each_field() {
    let update = ToolProgress::message("downloading")
        .with_fraction(0.5)
        .with_partial(json!({"bytes": 512}));
    assert!(!update.is_empty());
    assert_eq!(update.message.as_deref(), Some("downloading"));
    assert_eq!(update.fraction, Some(0.5));
    assert_eq!(update.partial, Some(json!({"bytes": 512})));
}

#[test]
fn fraction_is_clamped_to_the_unit_interval() {
    assert_eq!(
        ToolProgress::default().with_fraction(1.7).fraction,
        Some(1.0)
    );
    assert_eq!(
        ToolProgress::default().with_fraction(-3.0).fraction,
        Some(0.0)
    );
    assert_eq!(
        ToolProgress::default().with_fraction(f32::NAN).fraction,
        None
    );
}

#[test]
fn the_wire_form_omits_absent_fields() {
    let update = ToolProgress::message("halfway").with_fraction(0.5);
    assert_eq!(
        serde_json::to_value(&update).unwrap(),
        json!({"message": "halfway", "fraction": 0.5})
    );
    let back: ToolProgress = serde_json::from_value(json!({"message": "halfway"})).unwrap();
    assert_eq!(back, ToolProgress::message("halfway"));
}

#[test]
fn a_sink_delivers_every_update_to_its_closure() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = {
        let seen = seen.clone();
        ProgressSink::new(move |update| seen.lock().unwrap().push(update))
    };
    sink.report(ToolProgress::message("one"));
    sink.clone().report(ToolProgress::message("two"));
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen.iter().map(|u| u.message.clone()).collect::<Vec<_>>(),
        vec![Some("one".to_owned()), Some("two".to_owned())]
    );
}

#[test]
fn a_noop_sink_swallows_updates() {
    ProgressSink::noop().report(ToolProgress::message("ignored"));
    ProgressSink::default().report(ToolProgress::message("ignored"));
}

/// A context that overrides nothing: every existing implementor looks like this.
struct Bare;
impl ToolRunContext for Bare {}

#[test]
fn a_context_that_does_not_stream_ignores_progress() {
    let erased: &dyn ToolRunContext = &Bare;
    erased.report_progress(ToolProgress::message("nobody is listening"));
}

struct Streaming(ProgressSink);
impl ToolRunContext for Streaming {
    fn report_progress(&self, update: ToolProgress) {
        self.0.report(update);
    }
}

#[test]
fn a_streaming_context_receives_progress_through_the_trait_object() {
    let seen = Arc::new(Mutex::new(0usize));
    let counter = seen.clone();
    let ctx = Streaming(ProgressSink::new(move |_| *counter.lock().unwrap() += 1));
    let erased: &dyn ToolRunContext = &ctx;
    erased.report_progress(ToolProgress::message("a"));
    erased.report_progress(ToolProgress::message("b"));
    assert_eq!(*seen.lock().unwrap(), 2);
}

#[test]
fn a_sink_debug_reports_whether_it_is_connected() {
    let connected = format!("{:?}", ProgressSink::new(|_| {}));
    let noop = format!("{:?}", ProgressSink::noop());
    assert!(connected.contains("connected: true"), "{connected}");
    assert!(noop.contains("connected: false"), "{noop}");
}
