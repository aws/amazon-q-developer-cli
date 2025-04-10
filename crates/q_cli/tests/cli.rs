mod common;

use common::*;

// Integrations tests for the CLI
//
// This should be used to test interfaces that external code may rely on
// (exit codes, structured output, CLI flags)

#[test]
fn version_flag_has_status_code_zero() -> Result<()> {
    cli()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
    Ok(())
}

#[test]
fn help_flag_has_status_code_zero() -> Result<()> {
    cli().arg("--help").assert().success();
    Ok(())
}

#[test]
fn help_all_flag_has_status_code_zero() -> Result<()> {
    cli().arg("--help-all").assert().success();
    Ok(())
}
