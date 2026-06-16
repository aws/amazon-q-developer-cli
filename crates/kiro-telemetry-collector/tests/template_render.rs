//! Pure-Rust integration test: render the embedded template, parse with
//! serde_yaml, and assert structural invariants. Runs on every CI build (no
//! collector binary required).

use std::path::PathBuf;

use serde_yaml::Value;

#[test]
fn rendered_template_has_required_structure() {
    // The template is rendered through the public `config` API via reflection
    // on a small driver in this test file: we depend on the crate as a
    // library and call into a thin re-rendering path that mirrors what
    // `lifecycle::ensure_running` does.
    //
    // We avoid reaching into private modules by reproducing the substitution
    // here — the template itself is shipped as part of the crate at a known
    // path so we can read it via `include_str!`-equivalent in tests.

    // Read the template directly via CARGO_MANIFEST_DIR.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let template_path = PathBuf::from(&manifest_dir).join("src").join("collector.yaml.tmpl");
    let template = std::fs::read_to_string(&template_path).expect("read template");

    let rendered = template
        .replace("{{UPSTREAM_ENDPOINT}}", "https://upstream.example.com/v1/metrics")
        .replace("{{QUEUE_DIR}}", "/tmp/kiro-test-queue")
        .replace("{{CRASHES_DIR}}", "/tmp/kiro-test-crashes");

    let v: Value = serde_yaml::from_str(&rendered).expect("yaml parses");

    // receivers.otlp.protocols.http.endpoint == "127.0.0.1:14318"
    let otlp_endpoint = v
        .get("receivers")
        .and_then(|x| x.get("otlp"))
        .and_then(|x| x.get("protocols"))
        .and_then(|x| x.get("http"))
        .and_then(|x| x.get("endpoint"))
        .and_then(|x| x.as_str())
        .expect("otlp endpoint present");
    assert_eq!(otlp_endpoint, "127.0.0.1:14318");

    // exporters.otlphttp/upstream.endpoint matches our substitution.
    let upstream_endpoint = v
        .get("exporters")
        .and_then(|x| x.get("otlphttp/upstream"))
        .and_then(|x| x.get("endpoint"))
        .and_then(|x| x.as_str())
        .expect("upstream endpoint present");
    assert_eq!(upstream_endpoint, "https://upstream.example.com/v1/metrics");

    // service.pipelines.metrics.processors must include our cardinality
    // and high-cardinality filter processors.
    let processors = v
        .get("service")
        .and_then(|x| x.get("pipelines"))
        .and_then(|x| x.get("metrics"))
        .and_then(|x| x.get("processors"))
        .and_then(|x| x.as_sequence())
        .expect("metrics processors present");
    let processor_names: Vec<&str> = processors.iter().filter_map(|p| p.as_str()).collect();
    assert!(
        processor_names.contains(&"transform/cardinality"),
        "got: {processor_names:?}"
    );
    assert!(
        processor_names.contains(&"filter/known_high_cardinality"),
        "got: {processor_names:?}"
    );

    // extensions list includes file_storage/kiro and health_check.
    let extensions = v
        .get("service")
        .and_then(|x| x.get("extensions"))
        .and_then(|x| x.as_sequence())
        .expect("extensions present");
    let ext_names: Vec<&str> = extensions.iter().filter_map(|e| e.as_str()).collect();
    assert!(ext_names.contains(&"file_storage/kiro"), "got: {ext_names:?}");
    assert!(ext_names.contains(&"health_check"), "got: {ext_names:?}");

    // health_check.endpoint binds 127.0.0.1:13133.
    let health_endpoint = v
        .get("extensions")
        .and_then(|x| x.get("health_check"))
        .and_then(|x| x.get("endpoint"))
        .and_then(|x| x.as_str())
        .expect("health endpoint present");
    assert_eq!(health_endpoint, "127.0.0.1:13133");

    // file_storage queue dir flows through.
    let queue_dir = v
        .get("extensions")
        .and_then(|x| x.get("file_storage/kiro"))
        .and_then(|x| x.get("directory"))
        .and_then(|x| x.as_str())
        .expect("queue dir present");
    assert_eq!(queue_dir, "/tmp/kiro-test-queue");
}
