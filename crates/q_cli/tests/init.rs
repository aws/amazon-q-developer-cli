#[cfg(not(windows))]
use std::io::Write;
#[cfg(not(windows))]
use std::process::{
    Command,
    Stdio,
};

use anstream::println;
#[cfg(not(windows))]
use assert_cmd::prelude::*;
use eyre::Context;
use fig_util::consts::CLI_CRATE_NAME;
use fig_util::consts::build::{
    SKIP_FISH_TESTS,
    SKIP_SHELLCHECK_TESTS,
};
use paste::paste;

macro_rules! init_test {
    ($shell:expr, $stage:expr, $file:expr, [$exe:expr, $($arg:expr),*]) => {
        paste! {
            #[cfg(not(windows))]
            fn [<init_output_ $shell _ $stage _ $file>]() -> Result<String, Box<dyn std::error::Error>> {
                let mut cmd = Command::cargo_bin(CLI_CRATE_NAME)?;
                cmd.arg("init").arg($shell).arg($stage).arg("--rcfile").arg($file);
                cmd.env("Q_INIT_SNAPSHOT_TEST", "1");
                let out = cmd.assert().success().get_output().stdout.clone();
                Ok(String::from_utf8(out)?)
            }

            #[test]
            #[cfg(not(windows))]
            fn [<init_snapshot_ $shell _ $stage _ $file>]() -> Result<(), Box<dyn std::error::Error>> {
                let init = [<init_output_ $shell _ $stage _ $file>]()?;

                insta::assert_snapshot!(init);

                Ok(())
            }

            #[test]
            #[cfg(not(windows))]
            fn [<init_lint_ $shell _ $stage _ $file>]() -> Result<(), Box<dyn std::error::Error>> {
                // Ignore fish post since idk it doesn't work on CI
                if $exe == "fish" && $stage == "post" {
                    return Ok(());
                }

                if $exe == "fish" && SKIP_FISH_TESTS {
                    return Ok(());
                }

                if $exe == "shellcheck" && SKIP_SHELLCHECK_TESTS {
                    return Ok(());
                }

                let init = [<init_output_ $shell _ $stage _ $file>]()?;

                let mut cmd = Command::new($exe);
                cmd$(.arg($arg))*.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
                cmd.env("Q_INIT_SNAPSHOT_TEST", "1");

                let child = cmd.spawn().context(format!("{} is not installed", $exe))?;
                write!(child.stdin.as_ref().unwrap(), "{}", init)?;
                let output = child.wait_with_output()?;
                if !output.status.success() {
                    let stdout = String::from_utf8(output.stdout)?;
                    let stderr = String::from_utf8(output.stderr)?;
                    println!("stdout: {stdout}");
                    println!("stderr: {stderr}");

                    // Write shell version to stdout
                    let mut cmd = Command::new($exe);
                    cmd.arg("--version");
                    let out = cmd.output()?;
                    println!("Linter {} version: {}", $exe, String::from_utf8(out.stdout)?);

                    panic!(
                        "linter returned {}. please run `cargo run -p {CLI_CRATE_NAME} -- init {} {} --rcfile {} | {} {}`",
                        output.status, $shell, $stage, $file, $exe, [$($arg),*].join(" ")
                    );
                }

                Ok(())
            }
        }
    };
}

// bash
init_test!("bash", "pre", "bashrc", ["shellcheck", "-s", "bash", "-"]);
init_test!("bash", "pre", "bash_profile", ["shellcheck", "-s", "bash", "-"]);
init_test!("bash", "post", "bashrc", ["shellcheck", "-s", "bash", "-"]);
init_test!("bash", "post", "bash_profile", ["shellcheck", "-s", "bash", "-"]);

// zsh
init_test!("zsh", "pre", "zshrc", ["shellcheck", "-s", "bash", "-"]);
init_test!("zsh", "pre", "zprofile", ["shellcheck", "-s", "bash", "-"]);
init_test!("zsh", "post", "zshrc", ["shellcheck", "-s", "bash", "-"]);
init_test!("zsh", "post", "zprofile", ["shellcheck", "-s", "bash", "-"]);

// fish
init_test!("fish", "pre", "00_fig_pre", ["fish", "--no-execute"]);
init_test!("fish", "post", "99_fig_post", ["fish", "--no-execute"]);
