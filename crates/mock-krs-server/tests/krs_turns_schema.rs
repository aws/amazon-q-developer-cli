//! Keeps the checked-in KRS turn schema in step with the Rust types.
//!
//! `packages/tui/e2e_tests/smoke/krs-turns.schema.json` is what a scenario author
//! writes their `krs` block against, and what `scenarios.schema.json` references.
//! It is generated from [`mock_krs_server::scenario::KrsScript`] rather than
//! hand-written, because a hand-written copy drifts silently: the schema would
//! keep describing a field the server no longer reads.
//!
//! Regenerate with:
//!
//! ```text
//! UPDATE_KRS_SCHEMA=1 cargo test -p mock-krs-server --test krs_turns_schema
//! ```

use std::path::PathBuf;

use mock_krs_server::scenario::KrsScript;

fn schema_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/tui/e2e_tests/smoke/krs-turns.schema.json")
}

/// The schema as the current types describe it, with the metadata the smoke suite
/// expects at the top.
fn generated() -> String {
    let mut schema = serde_json::to_value(schemars::schema_for!(KrsScript)).expect("schema serializes");
    let object = schema.as_object_mut().expect("schema is an object");
    object.insert(
        "$id".to_string(),
        serde_json::json!("https://kiro.dev/schemas/krs-turns.schema.json"),
    );
    object.insert(
        "title".to_string(),
        serde_json::json!("Fake KRS turns for a smoke scenario"),
    );
    object.insert(
        "description".to_string(),
        serde_json::json!(
            "Generated from mock_krs_server::scenario::KrsScript. Do not edit by hand: run \
             `UPDATE_KRS_SCHEMA=1 cargo test -p mock-krs-server --test krs_turns_schema`."
        ),
    );
    format!(
        "{}\n",
        serde_json::to_string_pretty(&schema).expect("schema pretty-prints")
    )
}

#[test]
fn the_checked_in_schema_matches_the_rust_types() {
    let path = schema_path();
    let expected = generated();

    if std::env::var_os("UPDATE_KRS_SCHEMA").is_some() {
        std::fs::write(&path, &expected).expect("write the schema");
        return;
    }

    let actual = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{} is missing ({error}). Generate it with \
             `UPDATE_KRS_SCHEMA=1 cargo test -p mock-krs-server --test krs_turns_schema`",
            path.display()
        )
    });

    assert_eq!(
        actual, expected,
        "the checked-in KRS turn schema is stale. Regenerate it with \
         `UPDATE_KRS_SCHEMA=1 cargo test -p mock-krs-server --test krs_turns_schema`"
    );
}

#[test]
fn the_schema_describes_the_fields_a_scenario_author_writes() {
    // Cheap guard on the generated shape: if these move, the `krs` block in
    // scenarios.schema.json is describing something else.
    let schema: serde_json::Value = serde_json::from_str(&generated()).expect("valid json");

    assert_eq!(schema["properties"]["turns"]["type"], "array");
    let defs = &schema["$defs"];
    for definition in ["Turn", "Matcher", "Respond", "EventScript"] {
        assert!(!defs[definition].is_null(), "the schema no longer defines {definition}");
    }
    // `match` is the JSON name of `Turn::matcher`; a scenario writes that key.
    assert!(
        !defs["Turn"]["properties"]["match"].is_null(),
        "Turn no longer exposes `match`: {}",
        defs["Turn"]
    );
    // The event variants are tagged by `type`.
    assert_eq!(defs["EventScript"]["oneOf"][0]["properties"]["type"]["const"], "text");
}
