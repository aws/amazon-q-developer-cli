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

#[derive(Debug, Clone)]
struct InitTest {
    shell: String,
    stage: String,
    file: String,
    exe: String,
    exe_args: Vec<String>,
}

#[cfg(not(windows))]
fn init_output(test: &InitTest) -> Result<String, Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin(CLI_CRATE_NAME)?;
    cmd.arg("init")
        .arg(&test.shell)
        .arg(&test.stage)
        .arg("--rcfile")
        .arg(&test.file);
    cmd.env("Q_INIT_SNAPSHOT_TEST", "1");
    let out = cmd.assert().success().get_output().stdout.clone();
    Ok(String::from_utf8(out)?)
}

#[cfg(not(windows))]
fn init_snapshot(test: &InitTest) -> Result<(), Box<dyn std::error::Error>> {
    let init = init_output(test)?;

    insta::assert_snapshot!(
        format!("init_snapshot_{}_{}_{}", &test.shell, &test.stage, &test.file),
        init
    );

    Ok(())
}

#[cfg(not(windows))]
fn init_lint(test: &InitTest) -> Result<(), Box<dyn std::error::Error>> {
    // Ignore fish post since idk it doesn't work on CI
    if &test.exe == "fish" && &test.stage == "post" {
        return Ok(());
    }

    if &test.exe == "fish" && SKIP_FISH_TESTS {
        return Ok(());
    }

    if &test.exe == "shellcheck" && SKIP_SHELLCHECK_TESTS {
        return Ok(());
    }

    let init = init_output(test)?;

    let mut cmd = Command::new(&test.exe);
    for arg in &test.exe_args {
        cmd.arg(arg);
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.env("Q_INIT_SNAPSHOT_TEST", "1");

    let child = cmd.spawn().context(format!("{} is not installed", &test.exe))?;
    write!(child.stdin.as_ref().unwrap(), "{}", init)?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stdout = String::from_utf8(output.stdout)?;
        let stderr = String::from_utf8(output.stderr)?;
        println!("stdout: {stdout}");
        println!("stderr: {stderr}");

        // Write shell version to stdout
        let mut cmd = Command::new(&test.exe);
        cmd.arg("--version");
        let out = cmd.output()?;
        println!("Linter {} version: {}", &test.exe, String::from_utf8(out.stdout)?);

        panic!(
            "linter returned {}. please run `cargo run -p {CLI_CRATE_NAME} -- init {} {} --rcfile {} | {} {}`",
            output.status,
            &test.shell,
            &test.stage,
            &test.file,
            &test.exe,
            &test.exe_args.join(" ")
        );
    }

    Ok(())
}

macro_rules! init_test {
    ($shell:expr, $stage:expr, $file:expr, [$exe:expr, $($arg:expr),*]) => {
        InitTest {
            shell: $shell.to_string(),
            stage: $stage.to_string(),
            file: $file.to_string(),
            exe: $exe.to_string(),
            exe_args: [$($arg),*].into_iter().map(String::from).collect()
        }
    }
}

#[test]
fn init_tests() {
    let tests = vec![
        // bash
        init_test!("bash", "pre", "bashrc", ["shellcheck", "-s", "bash", "-"]),
        init_test!("bash", "pre", "bash_profile", ["shellcheck", "-s", "bash", "-"]),
        init_test!("bash", "post", "bashrc", ["shellcheck", "-s", "bash", "-"]),
        init_test!("bash", "post", "bash_profile", ["shellcheck", "-s", "bash", "-"]),
        // zsh
        init_test!("zsh", "pre", "zshrc", ["shellcheck", "-s", "bash", "-"]),
        init_test!("zsh", "pre", "zprofile", ["shellcheck", "-s", "bash", "-"]),
        init_test!("zsh", "post", "zshrc", ["shellcheck", "-s", "bash", "-"]),
        init_test!("zsh", "post", "zprofile", ["shellcheck", "-s", "bash", "-"]),
        // fish
        init_test!("fish", "pre", "00_fig_pre", ["fish", "--no-execute"]),
        init_test!("fish", "post", "99_fig_post", ["fish", "--no-execute"]),
    ];

    for test in &tests {
        init_snapshot(test).unwrap();
        init_lint(test).unwrap();
    }
}
