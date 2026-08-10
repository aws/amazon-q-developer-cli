#![cfg(unix)]

//! End-to-end coverage for the V3 ACP launcher.
//!
//! The hidden `acp --agent-engine=kas` command must leave the external ACP wire
//! untouched while it launches KAS with inherited stdio.

use std::os::unix::fs::PermissionsExt;

use assert_cmd::Command;
use chat_cli::util::consts::env_var::{
    KIRO_DATA_DIR,
    KIRO_KAS_NODE_PATH,
    KIRO_KAS_SERVER_PATH,
    KIRO_TEST_DB_PATH,
};
use serde_json::json;

#[test]
fn v3_acp_forwards_initialize_client_info_unchanged() {
    let home = tempfile::tempdir().expect("create temp home");
    let fake_node = home.path().join("fake-node");
    let fake_server = home.path().join("acp-server.js");
    let capture = home.path().join("initialize.json");

    std::fs::write(
        &fake_node,
        r#"#!/bin/sh
IFS= read -r request
printf '%s\n' "$request" > "$KIRO_TEST_CAPTURE_PATH"
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"mock-kas","version":"0.0.0"}}}'
"#,
    )
    .expect("write fake node");
    let mut permissions = std::fs::metadata(&fake_node).expect("stat fake node").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_node, permissions).expect("make fake node executable");
    std::fs::write(&fake_server, "").expect("write fake KAS server");

    let initialize = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientInfo": {
                "name": "Sugarmaker",
                "version": "3.0.0"
            },
            "clientCapabilities": {}
        }
    });

    Command::cargo_bin("chat_cli")
        .expect("locate chat_cli binary")
        .args(["acp", "--agent-engine=kas"])
        .env("HOME", home.path())
        .env("KIRO_TEST_MODE", "1")
        .env(KIRO_DATA_DIR, home.path().join("data"))
        .env(KIRO_TEST_DB_PATH, home.path().join("data.sqlite3"))
        .env(KIRO_KAS_NODE_PATH, &fake_node)
        .env(KIRO_KAS_SERVER_PATH, &fake_server)
        .env("KIRO_TEST_CAPTURE_PATH", &capture)
        .write_stdin(format!("{initialize}\n"))
        .assert()
        .success();

    let captured: serde_json::Value =
        serde_json::from_slice(&std::fs::read(capture).expect("read captured initialize"))
            .expect("parse captured initialize");
    assert_eq!(captured, initialize);
}
