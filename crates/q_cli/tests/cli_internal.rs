mod common;

use common::*;

#[test]
fn local_state_all() -> Result<()> {
    cli()
        .args(["_", "local-state", "all", "-f", "json"])
        .assert()
        .stdout(is_json())
        .success();
    Ok(())
}

#[test]
fn local_state_get() -> Result<()> {
    cli()
        .args(["_", "local-state", "test-value", "-f", "json"])
        .assert()
        .stdout(is_json())
        .success();
    Ok(())
}

#[test]
fn get_shell() -> Result<()> {
    cli().args(["_", "get-shell"]).assert().success();
    Ok(())
}

#[test]
fn hostname() -> Result<()> {
    cli().args(["_", "hostname"]).assert().success();
    Ok(())
}

#[test]
fn should_figterm_launch_code_success() -> Result<()> {
    cli()
        .args(["_", "should-figterm-launch"])
        .env("Q_FORCE_FIGTERM_LAUNCH", "1")
        .assert()
        .success();
    Ok(())
}

#[test]
fn should_figterm_launch_code_failure() -> Result<()> {
    cli().args(["_", "should-figterm-launch"]).assert().failure();
    Ok(())
}

#[test]
fn socket_dir() -> Result<()> {
    cli()
        .args(["_", "sockets-dir"])
        .assert()
        .stdout(format!("{}\n", fig_util::directories::sockets_dir_utf8().unwrap()))
        .success();
    Ok(())
}

#[test]
fn figterm_socket_path() -> Result<()> {
    cli()
        .args(["_", "figterm-socket-path", "abcd"])
        .assert()
        .stdout(format!(
            "{}\n",
            fig_util::directories::figterm_socket_path_utf8("abcd").unwrap()
        ))
        .success();
    Ok(())
}

#[test]
fn uuidgen() -> Result<()> {
    cli().args(["_", "uuidgen"]).assert().success();
    Ok(())
}
