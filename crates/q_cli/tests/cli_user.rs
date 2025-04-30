mod common;

use common::*;

#[ignore = "TODO: Fix json output"]
#[test]
fn user_whoami() -> Result<()> {
    cli().args(["user", "whoami"]).assert().code(predicate::in_iter([0, 1]));
    cli()
        .args(["user", "whoami", "-f", "json"])
        .assert()
        .stdout(is_json())
        .code(predicate::in_iter([0, 1]));
    Ok(())
}
