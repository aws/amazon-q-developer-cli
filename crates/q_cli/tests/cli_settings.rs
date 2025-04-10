mod common;

use common::*;

#[test]
fn settings_all() -> Result<()> {
    cli().args(["settings", "all"]).assert().success();
    cli()
        .args(["settings", "all", "-f", "json"])
        .assert()
        .stdout(is_json())
        .success();
    Ok(())
}

#[test]
fn settings_get() -> Result<()> {
    cli()
        .args(["settings", "test-value"])
        .assert()
        .code(predicate::in_iter([0, 1]));

    cli()
        .args(["settings", "test-value", "-f", "json"])
        .assert()
        .stdout(is_json())
        .success();
    Ok(())
}
