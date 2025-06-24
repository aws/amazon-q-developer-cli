use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

#[test]
fn version_flag_has_status_code_zero() {
    let mut cmd = Command::cargo_bin("figterm").unwrap();
    cmd.arg("--version");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}
