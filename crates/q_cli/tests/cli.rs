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
fn version_changelog_has_status_code_zero() -> Result<()> {
    cli()
        .arg("version")
        .arg("--changelog")
        .assert()
        .success()
        .stdout(predicate::str::contains("Changelog for version"));
    Ok(())
}

#[test]
fn version_changelog_all_has_status_code_zero() -> Result<()> {
    cli()
        .arg("version")
        .arg("--changelog=all")
        .assert()
        .success()
        .stdout(predicate::str::contains("Changelog for all versions"));
    Ok(())
}

#[test]
fn version_changelog_specific_has_status_code_zero() -> Result<()> {
    cli()
        .arg("version")
        .arg("--changelog=1.8.0")
        .assert()
        .success()
        .stdout(predicate::str::contains("Changelog for version 1.8.0"));
    Ok(())
}

#[test]
fn version_changelog_nonexistent_version() -> Result<()> {
    // Test with a version that's unlikely to exist
    cli()
        .arg("version")
        .arg("--changelog=999.999.999")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "No changelog information available for version 999.999.999",
        ));
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
