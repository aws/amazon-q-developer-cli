#![cfg(unix)]

//! End-to-end coverage for the V3 ACP launcher.
//!
//! Without an auth flag, the hidden v3 ACP command leaves the external wire
//! untouched. With `--auth-method=cli`, it consumes only KAS auth callbacks.

#[cfg(target_os = "linux")]
use std::fs::OpenOptions;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Stdio;
use std::time::{
    Duration,
    Instant,
};

use assert_cmd::Command;
#[cfg(target_os = "linux")]
use chat_cli::util::consts::env_var::{
    KIRO_CHAT_LOG_FILE,
    KIRO_LOG_LEVEL,
};
use chat_cli::util::consts::env_var::{
    KIRO_DATA_DIR,
    KIRO_KAS_NODE_PATH,
    KIRO_KAS_SERVER_PATH,
    KIRO_TEST_DB_PATH,
};
#[cfg(target_os = "linux")]
use rusqlite::{
    Connection,
    OptionalExtension as _,
    params,
};
use serde_json::json;
use tokio::io::{
    AsyncBufReadExt as _,
    AsyncReadExt as _,
    AsyncWriteExt as _,
    BufReader,
};

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

#[tokio::test]
async fn v3_acp_cli_auth_hides_callback_and_returns_login_error_to_kas() {
    let home = tempfile::tempdir().expect("create temp home");
    let fake_node = home.path().join("fake-node");
    let fake_server = home.path().join("acp-server.js");
    let initialize_capture = home.path().join("initialize.json");
    let auth_capture = home.path().join("auth-response.json");

    std::fs::write(
        &fake_node,
        r#"#!/bin/sh
set -eu
IFS= read -r initialize
printf '%s\n' "$initialize" > "$KIRO_TEST_INITIALIZE_CAPTURE_PATH"
printf '%s\n' '{"jsonrpc":"2.0","id":"auth-17","method":"_kiro/auth/getAccessToken","params":{}}'
IFS= read -r auth_response
printf '%s\n' "$auth_response" > "$KIRO_TEST_AUTH_CAPTURE_PATH"
printf '%s\n' '{"jsonrpc":"2.0","method":"session/update","params":{"status":"ready"}}'
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
                "name": "generic-acp-client",
                "version": "1.0.0"
            },
            "clientCapabilities": {}
        }
    });

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_chat_cli"))
        .args(["acp", "--agent-engine=v3", "--auth-method=cli"])
        .env("HOME", home.path())
        .env("KIRO_TEST_MODE", "1")
        .env(KIRO_DATA_DIR, home.path().join("data"))
        .env(KIRO_TEST_DB_PATH, home.path().join("data.sqlite3"))
        .env(KIRO_KAS_NODE_PATH, &fake_node)
        .env(KIRO_KAS_SERVER_PATH, &fake_server)
        .env("KIRO_TEST_INITIALIZE_CAPTURE_PATH", &initialize_capture)
        .env("KIRO_TEST_AUTH_CAPTURE_PATH", &auth_capture)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn chat_cli");

    let mut stdin = child.stdin.take().expect("capture chat_cli stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("capture chat_cli stdout"));
    stdin
        .write_all(format!("{initialize}\n").as_bytes())
        .await
        .expect("write initialize");
    stdin.flush().await.expect("flush initialize");

    // Keep stdin open until this ordinary frame proves fake KAS received the
    // CLI-generated auth response and advanced past the callback.
    let mut external_output = String::new();
    let bytes_read = tokio::time::timeout(Duration::from_secs(10), stdout.read_line(&mut external_output))
        .await
        .expect("timed out waiting for proxied ACP output")
        .expect("read proxied ACP output");
    assert_ne!(bytes_read, 0, "chat_cli stdout closed before the normal KAS frame");

    // Keep the parent side of stdin open while KAS exits. The CLI must not
    // wait for Tokio's uncancellable blocking stdin task during shutdown.
    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .expect("timed out waiting for chat_cli with parent stdin still open")
        .expect("wait for chat_cli");
    drop(stdin);
    assert!(status.success(), "chat_cli exited with {status}");
    stdout
        .read_to_string(&mut external_output)
        .await
        .expect("read remaining proxied ACP output");

    let captured_initialize: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&initialize_capture).expect("read captured initialize"))
            .expect("parse captured initialize");
    assert_eq!(captured_initialize, initialize);

    let auth_response: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&auth_capture).expect("read captured auth response"))
            .expect("parse captured auth response");
    assert_eq!(auth_response["id"], "auth-17");
    assert_eq!(auth_response["error"]["code"], -32603);
    assert_eq!(
        auth_response["error"]["data"]["details"],
        "You are not logged in. Please log in with `kiro-cli login`."
    );
    assert!(auth_response.get("result").is_none());

    let external_frames: Vec<serde_json::Value> = external_output
        .lines()
        .map(|line| serde_json::from_str(line).expect("external output is valid NDJSON"))
        .collect();
    assert_eq!(external_frames, vec![json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": { "status": "ready" }
    })]);
    assert!(!external_output.contains("_kiro/auth/getAccessToken"));
    assert!(!external_output.contains("accessToken"));
    assert!(!external_output.contains("kiro-cli login"));
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn v3_acp_cli_auth_drains_accepted_callback_after_client_eof() {
    const REFRESH_CLAIM_KEY: &str = "kirocli:auth:last-forced-refresh-at";
    const EOF_DRAIN_TRACE: &str = "external ACP stdin closed; draining accepted v3 engine responses";

    let home = tempfile::tempdir().expect("create temp home");
    let fake_node = home.path().join("fake-node");
    let fake_server = home.path().join("acp-server.js");
    let database = home.path().join("data.sqlite3");
    let data_local = home.path().join("data-local");
    let refresh_lock_path = data_local.join("kiro-cli/.refresh.lock");
    let engine_ready = home.path().join("engine-ready");
    let release_engine = home.path().join("release-engine");
    let auth_capture = home.path().join("auth-response.json");
    let engine_eof = home.path().join("engine-eof");
    let premature_eof = home.path().join("premature-eof");
    let log_file = home.path().join("relay.log");

    std::fs::write(
        &fake_node,
        r#"#!/bin/sh
set -eu
IFS= read -r initialize
: > "$KIRO_TEST_ENGINE_READY_PATH"
while [ ! -e "$KIRO_TEST_RELEASE_ENGINE_PATH" ]; do sleep 0.01; done
printf '%s\n' '{"jsonrpc":"2.0","id":"auth-after-eof","method":"_kiro/auth/getAccessToken","params":{"forceRefresh":true}}'
if ! IFS= read -r auth_response; then
    : > "$KIRO_TEST_PREMATURE_EOF_PATH"
    exit 2
fi
printf '%s\n' "$auth_response" > "$KIRO_TEST_AUTH_CAPTURE_PATH"
if IFS= read -r unexpected; then
    exit 3
fi
: > "$KIRO_TEST_ENGINE_EOF_PATH"
"#,
    )
    .expect("write fake node");
    make_executable(&fake_node);
    std::fs::write(&fake_server, "").expect("write fake v3 engine server");

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_chat_cli"))
        .args(["acp", "--agent-engine=v3", "--auth-method=cli"])
        .env("HOME", home.path())
        .env("XDG_DATA_HOME", &data_local)
        .env("KIRO_TEST_MODE", "1")
        .env(KIRO_DATA_DIR, home.path().join("data"))
        .env(KIRO_TEST_DB_PATH, &database)
        .env(KIRO_KAS_NODE_PATH, &fake_node)
        .env(KIRO_KAS_SERVER_PATH, &fake_server)
        .env(KIRO_CHAT_LOG_FILE, &log_file)
        .env(KIRO_LOG_LEVEL, "chat_cli=debug")
        .env("KIRO_TEST_ENGINE_READY_PATH", &engine_ready)
        .env("KIRO_TEST_RELEASE_ENGINE_PATH", &release_engine)
        .env("KIRO_TEST_AUTH_CAPTURE_PATH", &auth_capture)
        .env("KIRO_TEST_ENGINE_EOF_PATH", &engine_eof)
        .env("KIRO_TEST_PREMATURE_EOF_PATH", &premature_eof)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn chat_cli");

    let mut stdin = child.stdin.take().expect("capture chat_cli stdin");
    let mut stdout = child.stdout.take().expect("capture chat_cli stdout");
    let mut stderr = child.stderr.take().expect("capture chat_cli stderr");
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n")
        .await
        .expect("write initialize");
    stdin.flush().await.expect("flush initialize");
    wait_for_file(&engine_ready, &mut child, "fake engine readiness").await;

    let expired_token = json!({
        "access_token": "expired-access-token",
        "expires_at": "2000-01-01T00:00:00Z",
        "refresh_token": null,
        "region": "us-east-1",
        "start_url": null,
        "oauth_flow": "DeviceCode",
        "scopes": null
    });
    let connection = Connection::open(&database).expect("open migrated auth database");
    connection
        .execute("INSERT OR REPLACE INTO auth_kv (key, value) VALUES (?1, ?2)", params![
            "kirocli:odic:token",
            expired_token.to_string()
        ])
        .expect("seed expired Builder ID token");
    drop(connection);

    std::fs::create_dir_all(refresh_lock_path.parent().expect("refresh lock parent"))
        .expect("create refresh lock directory");
    let refresh_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&refresh_lock_path)
        .expect("open refresh lock");
    let mut refresh_lock = fd_lock::RwLock::new(refresh_file);
    let refresh_guard = refresh_lock.try_write().expect("hold refresh lock");
    std::fs::write(&release_engine, "").expect("release fake engine callback");

    let claim_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let claimed = Connection::open(&database)
            .and_then(|connection| {
                connection
                    .query_row("SELECT value FROM auth_kv WHERE key = ?1", [REFRESH_CLAIM_KEY], |row| {
                        row.get::<_, String>(0)
                    })
                    .optional()
            })
            .ok()
            .flatten()
            .is_some();
        if claimed {
            break;
        }
        if let Some(status) = child
            .try_wait()
            .expect("inspect chat_cli while waiting for callback acceptance")
        {
            panic!("chat_cli exited with {status} before accepting the auth callback");
        }
        assert!(
            Instant::now() < claim_deadline,
            "timed out waiting for callback acceptance"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    drop(stdin);
    let trace_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if std::fs::read_to_string(&log_file).is_ok_and(|log| log.contains(EOF_DRAIN_TRACE)) {
            break;
        }
        if let Some(status) = child.try_wait().expect("inspect chat_cli while waiting for EOF drain") {
            panic!("chat_cli exited with {status} before entering EOF drain");
        }
        assert!(
            Instant::now() < trace_deadline,
            "timed out waiting for production EOF drain"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(
        !premature_eof.exists(),
        "engine stdin closed before the accepted response completed"
    );
    assert!(
        !auth_capture.exists(),
        "the auth response completed before the drain, so the drain wait was never exercised"
    );

    drop(refresh_guard);
    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .expect("timed out waiting for chat_cli")
        .expect("wait for chat_cli");
    let mut external_output = String::new();
    stdout
        .read_to_string(&mut external_output)
        .await
        .expect("read external ACP output");
    let mut error_output = String::new();
    stderr
        .read_to_string(&mut error_output)
        .await
        .expect("read chat_cli stderr");
    assert!(status.success(), "chat_cli exited with {status}: {error_output}");

    let auth_response: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&auth_capture).expect("read captured auth response"))
            .expect("parse captured auth response");
    assert_eq!(auth_response["id"], "auth-after-eof");
    assert_eq!(auth_response["error"]["code"], -32603);
    assert_eq!(
        auth_response["error"]["data"]["details"],
        "You are not logged in. Please log in with `kiro-cli login`."
    );
    assert!(
        engine_eof.exists(),
        "fake engine did not receive real EOF after the response"
    );
    assert!(!premature_eof.exists(), "fake engine observed EOF before the response");
    assert!(
        external_output.is_empty(),
        "auth traffic escaped to ACP stdout: {external_output}"
    );
}

#[test]
fn v3_acp_cli_auth_delivers_real_eof_to_kas() {
    let home = tempfile::tempdir().expect("create temp home");
    let fake_node = home.path().join("fake-node");
    let fake_server = home.path().join("acp-server.js");
    let capture = home.path().join("stdin.ndjson");

    std::fs::write(
        &fake_node,
        r#"#!/bin/sh
cat > "$KIRO_TEST_CAPTURE_PATH"
"#,
    )
    .expect("write fake node");
    let mut permissions = std::fs::metadata(&fake_node).expect("stat fake node").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_node, permissions).expect("make fake node executable");
    std::fs::write(&fake_server, "").expect("write fake KAS server");

    let frame = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n";
    Command::cargo_bin("chat_cli")
        .expect("locate chat_cli binary")
        .args(["acp", "--agent-engine=v3", "--auth-method=cli"])
        .env("HOME", home.path())
        .env("KIRO_TEST_MODE", "1")
        .env(KIRO_DATA_DIR, home.path().join("data"))
        .env(KIRO_TEST_DB_PATH, home.path().join("data.sqlite3"))
        .env(KIRO_KAS_NODE_PATH, &fake_node)
        .env(KIRO_KAS_SERVER_PATH, &fake_server)
        .env("KIRO_TEST_CAPTURE_PATH", &capture)
        .write_stdin(frame)
        .assert()
        .success();

    assert_eq!(std::fs::read_to_string(capture).expect("read KAS stdin"), frame);
}

#[test]
fn v3_acp_cli_auth_bounds_kas_shutdown_after_external_eof() {
    let home = tempfile::tempdir().expect("create temp home");
    let fake_node = home.path().join("fake-node");
    let fake_server = home.path().join("acp-server.js");
    let descendant_pid = home.path().join("descendant.pid");

    std::fs::write(
        &fake_node,
        r#"#!/bin/sh
cat >/dev/null
sleep 30 &
printf '%s\n' "$!" > "$KIRO_TEST_DESCENDANT_PID_PATH"
wait
"#,
    )
    .expect("write fake node");
    let mut permissions = std::fs::metadata(&fake_node).expect("stat fake node").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_node, permissions).expect("make fake node executable");
    std::fs::write(&fake_server, "").expect("write fake KAS server");

    Command::cargo_bin("chat_cli")
        .expect("locate chat_cli binary")
        .args(["acp", "--agent-engine=v3", "--auth-method=cli"])
        .env("HOME", home.path())
        .env("KIRO_TEST_MODE", "1")
        .env(KIRO_DATA_DIR, home.path().join("data"))
        .env(KIRO_TEST_DB_PATH, home.path().join("data.sqlite3"))
        .env(KIRO_KAS_NODE_PATH, &fake_node)
        .env(KIRO_KAS_SERVER_PATH, &fake_server)
        .env("KIRO_TEST_DESCENDANT_PID_PATH", &descendant_pid)
        .write_stdin("{\"jsonrpc\":\"2.0\",\"method\":\"cancel\",\"params\":{}}\n")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "the v3 engine did not exit within 5 seconds after ACP stdin closed",
        ));

    let pid = read_pid(&descendant_pid);
    let deadline = Instant::now() + Duration::from_secs(3);
    while process_exists(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!process_exists(pid), "KAS descendant {pid} survived forced shutdown");
}

#[test]
fn default_v3_acp_keeps_auth_callback_and_response_on_external_wire() {
    let home = tempfile::tempdir().expect("create temp home");
    let fake_node = home.path().join("fake-node");
    let fake_server = home.path().join("acp-server.js");
    let response_capture = home.path().join("external-auth-response.ndjson");
    let callback = " {\"jsonrpc\":\"2.0\",\"id\":71,\"method\":\"_kiro/auth/getAccessToken\",\"params\":{}} \n";
    let response = "\t{\"jsonrpc\":\"2.0\",\"id\":71,\"result\":{\"accessToken\":\"external-token\"}}\n";

    std::fs::write(
        &fake_node,
        r#"#!/bin/sh
printf '%s' "$KIRO_TEST_CALLBACK"
IFS= read -r response
printf '%s\n' "$response" > "$KIRO_TEST_RESPONSE_CAPTURE"
"#,
    )
    .expect("write fake node");
    make_executable(&fake_node);
    std::fs::write(&fake_server, "").expect("write fake KAS server");

    Command::cargo_bin("chat_cli")
        .expect("locate chat_cli binary")
        .args(["acp", "--agent-engine=v3"])
        .env("HOME", home.path())
        .env("KIRO_TEST_MODE", "1")
        .env(KIRO_DATA_DIR, home.path().join("data"))
        .env(KIRO_TEST_DB_PATH, home.path().join("data.sqlite3"))
        .env(KIRO_KAS_NODE_PATH, &fake_node)
        .env(KIRO_KAS_SERVER_PATH, &fake_server)
        .env("KIRO_TEST_CALLBACK", callback)
        .env("KIRO_TEST_RESPONSE_CAPTURE", &response_capture)
        .write_stdin(response)
        .assert()
        .success()
        .stdout(predicates::str::diff(callback));

    assert_eq!(
        std::fs::read_to_string(response_capture).expect("read external auth response"),
        response
    );
}

#[test]
fn cli_auth_is_rejected_by_non_v3_dispatch() {
    for engine in [None, Some("v1"), Some("v2")] {
        let home = tempfile::tempdir().expect("create temp home");
        let mut command = Command::cargo_bin("chat_cli").expect("locate chat_cli binary");
        command.arg("acp");
        if let Some(engine) = engine {
            command.arg(format!("--agent-engine={engine}"));
        }
        command
            .arg("--auth-method=cli")
            .env("HOME", home.path())
            .env("KIRO_TEST_MODE", "1")
            .env(KIRO_DATA_DIR, home.path().join("data"))
            .env(KIRO_TEST_DB_PATH, home.path().join("data.sqlite3"))
            .assert()
            .failure()
            .stderr(predicates::str::contains(
                "--auth-method is only supported with --agent-engine=v3",
            ));
    }
}

#[tokio::test]
async fn cli_auth_flushes_a_publishing_response_before_external_eof() {
    let home = tempfile::tempdir().expect("create temp home");
    let fake_node = home.path().join("fake-node");
    let fake_server = home.path().join("acp-server.js");
    let response_capture = home.path().join("queued-response.ndjson");

    std::fs::write(
        &fake_node,
        r#"#!/bin/sh
printf '%s' '{"jsonrpc":"2.0","id":77,"method":"_kiro/other","params":{"padding":"'
head -c 1048576 /dev/zero | tr '\000' x
printf '%s\n' '"}}'
IFS= read -r response
printf '%s\n' "$response" > "$KIRO_TEST_RESPONSE_CAPTURE"
"#,
    )
    .expect("write fake node");
    make_executable(&fake_node);
    std::fs::write(&fake_server, "").expect("write fake KAS server");

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_chat_cli"))
        .args(["acp", "--agent-engine=v3", "--auth-method=cli"])
        .env("HOME", home.path())
        .env("KIRO_TEST_MODE", "1")
        .env(KIRO_DATA_DIR, home.path().join("data"))
        .env(KIRO_TEST_DB_PATH, home.path().join("data.sqlite3"))
        .env(KIRO_KAS_NODE_PATH, &fake_node)
        .env(KIRO_KAS_SERVER_PATH, &fake_server)
        .env("KIRO_TEST_RESPONSE_CAPTURE", &response_capture)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn chat_cli");
    let mut stdin = child.stdin.take().expect("capture chat_cli stdin");
    let mut stdout = child.stdout.take().expect("capture chat_cli stdout");
    let mut prefix = vec![0; 256];
    tokio::time::timeout(Duration::from_secs(10), stdout.read_exact(&mut prefix))
        .await
        .expect("timed out waiting for reverse-request prefix")
        .expect("read reverse-request prefix");
    let prefix = String::from_utf8(prefix).expect("reverse-request prefix is UTF-8");
    assert!(prefix.contains("\"id\":77"));
    assert!(prefix.contains("\"method\":\"_kiro/other\""));

    let response = b"{\"jsonrpc\":\"2.0\",\"id\":77,\"result\":{\"ok\":true}}\n";
    stdin.write_all(response).await.expect("write racing response");
    stdin.flush().await.expect("flush racing response");
    drop(stdin);
    let status = tokio::time::timeout(Duration::from_secs(10), async {
        let mut remainder = Vec::new();
        stdout.read_to_end(&mut remainder).await.expect("drain reverse request");
        child.wait().await.expect("wait for chat_cli")
    })
    .await
    .expect("queued response left chat_cli running");
    assert!(status.success(), "chat_cli exited with {status}");

    assert_eq!(
        std::fs::read(&response_capture).expect("read queued KAS response"),
        response
    );
}

#[tokio::test]
async fn cli_auth_bounds_a_blocked_kas_stdin_write() {
    let home = tempfile::tempdir().expect("create temp home");
    let fake_node = home.path().join("fake-node");
    let fake_server = home.path().join("acp-server.js");

    std::fs::write(
        &fake_node,
        r#"#!/bin/sh
exec sleep 30
"#,
    )
    .expect("write fake node");
    make_executable(&fake_node);
    std::fs::write(&fake_server, "").expect("write fake KAS server");

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_chat_cli"))
        .args(["acp", "--agent-engine=v3", "--auth-method=cli"])
        .env("HOME", home.path())
        .env("KIRO_TEST_MODE", "1")
        .env(KIRO_DATA_DIR, home.path().join("data"))
        .env(KIRO_TEST_DB_PATH, home.path().join("data.sqlite3"))
        .env(KIRO_KAS_NODE_PATH, &fake_node)
        .env(KIRO_KAS_SERVER_PATH, &fake_server)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn chat_cli");
    let mut stdin = child.stdin.take().expect("capture chat_cli stdin");
    let frame = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"padding\":\"{}\"}}}}\n",
        "x".repeat(1024 * 1024)
    );
    stdin.write_all(frame.as_bytes()).await.expect("write large ACP frame");
    drop(stdin);

    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .expect("blocked KAS write left chat_cli running")
        .expect("wait for chat_cli");
    assert!(!status.success(), "blocked KAS write unexpectedly succeeded");
}

#[tokio::test]
async fn cli_auth_cleans_detached_descendant_after_normal_kas_exit() {
    let home = tempfile::tempdir().expect("create temp home");
    let fake_node = home.path().join("fake-node");
    let fake_server = home.path().join("acp-server.js");
    let descendant_pid = home.path().join("detached.pid");

    std::fs::write(
        &fake_node,
        r#"#!/bin/sh
"$KIRO_TEST_PYTHON_PATH" -c 'import os,time; os.setsid(); os.close(0); os.close(1); os.close(2); fd=os.open(os.environ["KIRO_TEST_DESCENDANT_PID_PATH"], os.O_WRONLY|os.O_CREAT|os.O_TRUNC, 0o600); os.write(fd, str(os.getpid()).encode()); os.close(fd); time.sleep(30)' &
while [ ! -s "$KIRO_TEST_DESCENDANT_PID_PATH" ]; do sleep 0.001; done
exit 0
"#,
    )
    .expect("write fake node");
    make_executable(&fake_node);
    std::fs::write(&fake_server, "").expect("write fake KAS server");

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_chat_cli"))
        .args(["acp", "--agent-engine=v3", "--auth-method=cli"])
        .env("HOME", home.path())
        .env("KIRO_TEST_MODE", "1")
        .env(KIRO_DATA_DIR, home.path().join("data"))
        .env(KIRO_TEST_DB_PATH, home.path().join("data.sqlite3"))
        .env(KIRO_KAS_NODE_PATH, &fake_node)
        .env(KIRO_KAS_SERVER_PATH, &fake_server)
        .env("KIRO_TEST_DESCENDANT_PID_PATH", &descendant_pid)
        .env("KIRO_TEST_PYTHON_PATH", python3_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn chat_cli");
    let stdin = child.stdin.take().expect("keep chat_cli stdin open");
    let detached_pid = wait_for_pid(&descendant_pid, &mut child).await;
    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .expect("normal KAS exit left chat_cli running")
        .expect("wait for chat_cli");
    drop(stdin);

    assert!(status.success(), "chat_cli exited with {status}");
    assert_process_stops(detached_pid, "detached KAS descendant");
}

#[tokio::test]
async fn cli_auth_sigint_cleans_kas_and_detached_descendant() {
    let home = tempfile::tempdir().expect("create temp home");
    let fake_node = home.path().join("fake-node");
    let fake_server = home.path().join("acp-server.js");
    let descendant_pid = home.path().join("detached.pid");

    std::fs::write(
        &fake_node,
        r#"#!/bin/sh
"$KIRO_TEST_PYTHON_PATH" -c 'import os,time; os.setsid(); os.close(0); os.close(1); os.close(2); fd=os.open(os.environ["KIRO_TEST_DESCENDANT_PID_PATH"], os.O_WRONLY|os.O_CREAT|os.O_TRUNC, 0o600); os.write(fd, str(os.getpid()).encode()); os.close(fd); time.sleep(30)' &
while [ ! -s "$KIRO_TEST_DESCENDANT_PID_PATH" ]; do sleep 0.01; done
while :; do sleep 1; done
"#,
    )
    .expect("write fake node");
    make_executable(&fake_node);
    std::fs::write(&fake_server, "").expect("write fake KAS server");

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_chat_cli"))
        .args(["acp", "--agent-engine=v3", "--auth-method=cli"])
        .env("HOME", home.path())
        .env("KIRO_TEST_MODE", "1")
        .env(KIRO_DATA_DIR, home.path().join("data"))
        .env(KIRO_TEST_DB_PATH, home.path().join("data.sqlite3"))
        .env(KIRO_KAS_NODE_PATH, &fake_node)
        .env(KIRO_KAS_SERVER_PATH, &fake_server)
        .env("KIRO_TEST_DESCENDANT_PID_PATH", &descendant_pid)
        .env("KIRO_TEST_PYTHON_PATH", python3_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn chat_cli");
    let stdin = child.stdin.take().expect("keep chat_cli stdin open");
    let detached_pid = wait_for_pid(&descendant_pid, &mut child).await;

    let result = unsafe { libc::kill(child.id().expect("chat_cli pid") as i32, libc::SIGINT) };
    assert_eq!(result, 0, "send SIGINT: {}", std::io::Error::last_os_error());
    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .expect("SIGINT left chat_cli running")
        .expect("wait for chat_cli");
    drop(stdin);
    assert_eq!(status.code(), Some(130));
    assert_process_stops(detached_pid, "SIGINT-detached KAS descendant");
}

#[tokio::test]
async fn cli_auth_tracks_known_descendant_after_kas_root_exit() {
    let home = tempfile::tempdir().expect("create temp home");
    let fake_node = home.path().join("fake-node");
    let fake_server = home.path().join("acp-server.js");
    let known_pid = home.path().join("known.pid");
    let descendant_pid = home.path().join("detached-child.pid");

    std::fs::write(
        &fake_node,
        r#"#!/bin/sh
"$KIRO_TEST_PYTHON_PATH" -c '
import os
import signal
import time

signal.signal(signal.SIGTERM, signal.SIG_IGN)
root = os.getppid()
os.setsid()
with open(os.environ["KIRO_TEST_KNOWN_PID_PATH"], "w") as ready:
    ready.write(str(os.getpid()))
while os.getppid() == root:
    time.sleep(0.001)
child = os.fork()
if child == 0:
    os.setsid()
    os.close(0)
    os.close(1)
    os.close(2)
    fd = os.open(os.environ["KIRO_TEST_DESCENDANT_PID_PATH"], os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    os.write(fd, str(os.getpid()).encode())
    os.close(fd)
    time.sleep(30)
    os._exit(0)
time.sleep(0.2)
os._exit(0)
' &
while [ ! -s "$KIRO_TEST_KNOWN_PID_PATH" ]; do sleep 0.001; done
exit 0
"#,
    )
    .expect("write fake node");
    make_executable(&fake_node);
    std::fs::write(&fake_server, "").expect("write fake KAS server");

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_chat_cli"))
        .args(["acp", "--agent-engine=v3", "--auth-method=cli"])
        .env("HOME", home.path())
        .env("KIRO_TEST_MODE", "1")
        .env(KIRO_DATA_DIR, home.path().join("data"))
        .env(KIRO_TEST_DB_PATH, home.path().join("data.sqlite3"))
        .env(KIRO_KAS_NODE_PATH, &fake_node)
        .env(KIRO_KAS_SERVER_PATH, &fake_server)
        .env("KIRO_TEST_KNOWN_PID_PATH", &known_pid)
        .env("KIRO_TEST_DESCENDANT_PID_PATH", &descendant_pid)
        .env("KIRO_TEST_PYTHON_PATH", python3_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn chat_cli");
    let stdin = child.stdin.take().expect("keep chat_cli stdin open");
    let _known_pid = wait_for_pid(&known_pid, &mut child).await;
    let detached_pid = wait_for_pid(&descendant_pid, &mut child).await;
    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .expect("post-root-exit descendant left chat_cli running")
        .expect("wait for chat_cli");
    drop(stdin);

    assert!(status.success(), "chat_cli exited with {status}");
    assert_process_stops(detached_pid, "post-root-exit KAS descendant");
}

#[tokio::test]
async fn cli_auth_tracks_descendant_spawned_during_termination() {
    let home = tempfile::tempdir().expect("create temp home");
    let fake_node = home.path().join("fake-node");
    let fake_server = home.path().join("acp-server.js");
    let ready_pid = home.path().join("ready.pid");
    let descendant_pid = home.path().join("term-child.pid");

    std::fs::write(
        &fake_node,
        r#"#!/bin/sh
exec "$KIRO_TEST_PYTHON_PATH" -c '
import os
import signal
import time

spawned = False
def handle_term(_signal, _frame):
    global spawned
    if spawned:
        return
    spawned = True
    child = os.fork()
    if child == 0:
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        os.setsid()
        os.close(0)
        os.close(1)
        os.close(2)
        fd = os.open(os.environ["KIRO_TEST_DESCENDANT_PID_PATH"], os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        os.write(fd, str(os.getpid()).encode())
        os.close(fd)
        time.sleep(30)
        os._exit(0)

signal.signal(signal.SIGTERM, handle_term)
with open(os.environ["KIRO_TEST_READY_PID_PATH"], "w") as ready:
    ready.write(str(os.getpid()))
while True:
    time.sleep(1)
'
"#,
    )
    .expect("write fake node");
    make_executable(&fake_node);
    std::fs::write(&fake_server, "").expect("write fake KAS server");

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_chat_cli"))
        .args(["acp", "--agent-engine=v3", "--auth-method=cli"])
        .env("HOME", home.path())
        .env("KIRO_TEST_MODE", "1")
        .env(KIRO_DATA_DIR, home.path().join("data"))
        .env(KIRO_TEST_DB_PATH, home.path().join("data.sqlite3"))
        .env(KIRO_KAS_NODE_PATH, &fake_node)
        .env(KIRO_KAS_SERVER_PATH, &fake_server)
        .env("KIRO_TEST_READY_PID_PATH", &ready_pid)
        .env("KIRO_TEST_DESCENDANT_PID_PATH", &descendant_pid)
        .env("KIRO_TEST_PYTHON_PATH", python3_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn chat_cli");
    let stdin = child.stdin.take().expect("keep chat_cli stdin open");
    let _root_pid = wait_for_pid(&ready_pid, &mut child).await;

    let result = unsafe { libc::kill(child.id().expect("chat_cli pid") as i32, libc::SIGINT) };
    assert_eq!(result, 0, "send SIGINT: {}", std::io::Error::last_os_error());
    let detached_pid = wait_for_pid(&descendant_pid, &mut child).await;
    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .expect("termination-spawned descendant left chat_cli running")
        .expect("wait for chat_cli");
    drop(stdin);

    assert_eq!(status.code(), Some(130));
    assert_process_stops(detached_pid, "termination-spawned KAS descendant");
}

fn python3_path() -> std::path::PathBuf {
    [
        "/usr/bin/python3",
        "/usr/local/bin/python3",
        "/opt/homebrew/bin/python3",
    ]
    .into_iter()
    .map(std::path::PathBuf::from)
    .find(|path| path.is_file())
    .expect("a concrete Python 3 executable is required for Unix process-tree tests")
}

fn make_executable(path: &Path) {
    let mut permissions = std::fs::metadata(path).expect("stat fake node").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).expect("make fake node executable");
}

fn read_pid(path: &Path) -> i32 {
    std::fs::read_to_string(path)
        .expect("read descendant pid")
        .trim()
        .parse()
        .expect("parse descendant pid")
}

#[cfg(target_os = "linux")]
async fn wait_for_file(path: &Path, child: &mut tokio::process::Child, description: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if path.exists() {
            return;
        }
        if let Some(status) = child.try_wait().expect("inspect chat_cli while waiting for file") {
            panic!("chat_cli exited with {status} before {description}");
        }
        assert!(Instant::now() < deadline, "timed out waiting for {description}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_for_pid(path: &Path, child: &mut tokio::process::Child) -> i32 {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(pid) = std::fs::read_to_string(path)
            .ok()
            .and_then(|contents| contents.trim().parse().ok())
        {
            return pid;
        }
        if let Some(status) = child.try_wait().expect("inspect chat_cli while waiting for descendant") {
            panic!("chat_cli exited with {status} before detached descendant became ready");
        }
        assert!(Instant::now() < deadline, "timed out waiting for descendant pid");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn assert_process_stops(pid: i32, description: &str) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while process_exists(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!process_exists(pid), "{description} {pid} survived cleanup");
}

fn process_exists(pid: i32) -> bool {
    let result = unsafe { libc::kill(pid, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}
