//! Unit tests for `PermissionLevel`: its total order, default, `Display`
//! form, and the exact wire representation each variant is pinned to.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::PermissionLevel;

#[test]
fn levels_are_totally_ordered_from_none_to_dangerous() {
    // Enforcement compares levels with `<` to reject a tool whose required
    // level exceeds the caller's maximum, so this ordering is load-bearing.
    assert!(PermissionLevel::None < PermissionLevel::ReadOnly);
    assert!(PermissionLevel::ReadOnly < PermissionLevel::Write);
    assert!(PermissionLevel::Write < PermissionLevel::Execute);
    assert!(PermissionLevel::Execute < PermissionLevel::Dangerous);
}

#[test]
fn default_is_read_only() {
    assert_eq!(PermissionLevel::default(), PermissionLevel::ReadOnly);
}

#[test]
fn display_matches_the_variant_name() {
    assert_eq!(PermissionLevel::None.to_string(), "None");
    assert_eq!(PermissionLevel::ReadOnly.to_string(), "ReadOnly");
    assert_eq!(PermissionLevel::Write.to_string(), "Write");
    assert_eq!(PermissionLevel::Execute.to_string(), "Execute");
    assert_eq!(PermissionLevel::Dangerous.to_string(), "Dangerous");
}

#[test]
fn levels_round_trip_through_json() {
    for level in [
        PermissionLevel::None,
        PermissionLevel::ReadOnly,
        PermissionLevel::Write,
        PermissionLevel::Execute,
        PermissionLevel::Dangerous,
    ] {
        let encoded = serde_json::to_string(&level).expect("serializable");
        let back: PermissionLevel = serde_json::from_str(&encoded).expect("deserializable");
        assert_eq!(back, level);
    }
}

#[test]
fn levels_are_pinned_to_their_exact_wire_names() {
    // A round-trip alone only proves the encoder and decoder currently agree
    // with *each other* — it still passes if a `serde` rename quietly changes
    // what gets persisted. Pin the literal strings, in both directions, so a
    // rename that would silently corrupt an on-disk transcript or an RPC
    // payload already carrying `"ReadOnly"` fails here instead.
    let cases = [
        (PermissionLevel::None, "\"None\""),
        (PermissionLevel::ReadOnly, "\"ReadOnly\""),
        (PermissionLevel::Write, "\"Write\""),
        (PermissionLevel::Execute, "\"Execute\""),
        (PermissionLevel::Dangerous, "\"Dangerous\""),
    ];
    for (level, wire) in cases {
        assert_eq!(serde_json::to_string(&level).expect("serializable"), wire);
        assert_eq!(
            serde_json::from_str::<PermissionLevel>(wire).expect("deserializable"),
            level
        );
    }
}
