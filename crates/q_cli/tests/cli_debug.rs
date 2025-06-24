mod common;

use common::*;

#[test]
fn debug_root() -> Result<()> {
    cli().arg("debug").assert().code(predicate::eq(2));
    Ok(())
}

#[test]
#[cfg(target_os = "macos")]
fn debug_verify_codesign() -> Result<()> {
    cli()
        .args(["debug", "verify-codesign"])
        .assert()
        .code(predicate::in_iter([0, 1]));
    Ok(())
}

#[test]
fn debug_get_index() -> Result<()> {
    cli()
        .args(["debug", "get-index", "stable"])
        .assert()
        .code(predicate::in_iter([0, 1]));
    Ok(())
}

#[test]
fn debug_list_intellij_variants() -> Result<()> {
    cli()
        .args(["debug", "list-intellij-variants"])
        .assert()
        .code(predicate::in_iter([0, 1]));
    Ok(())
}

#[test]
fn debug_refresh_auth_token() -> Result<()> {
    cli()
        .args(["debug", "refresh-auth-token"])
        .assert()
        .code(predicate::in_iter([0, 1]));
    Ok(())
}
