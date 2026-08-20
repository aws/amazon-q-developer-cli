//! Contract tests for `docker/entrypoint.sh`.
//!
//! Ported from the Kiro-botCDK package's `runtime-contracts.test.ts` when the
//! script moved into this crate. The script is security-relevant — it performs
//! the privilege drop, generates `secrets.toml`, and substitutes Slack identity
//! values into the bot's config and Cedar policy — and the container health
//! check cannot detect any of those going wrong: `/healthz` returns a bare 200
//! with no dependency on Slack, ACP, or MCP.
//!
//! These run the real script under `bash` with fakes on `PATH`, so they need no
//! container and no privileges.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{
    Path,
    PathBuf,
};
use std::process::{
    Command,
    Output,
};

const SOURCE_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const GIT_URL: &str = "https://example.invalid/kiro-cli.git";

fn entrypoint() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("docker/entrypoint.sh")
}

fn write_exec(path: &Path, body: &str) {
    fs::write(path, body).expect("write script");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod");
}

/// Config source dir shaped like `/etc/kiro-help` in the built image: templated
/// `config.toml`, templated `policies/agents.cedar`, and a `source-sha`.
fn seed_config(root: &Path, config: &str, policy: &str, source_sha: Option<&str>) -> PathBuf {
    let dir = root.join("config");
    fs::create_dir_all(dir.join("policies")).expect("mkdir policies");
    fs::write(dir.join("config.toml"), config).expect("write config.toml");
    fs::write(dir.join("policies/agents.cedar"), policy).expect("write agents.cedar");
    if let Some(sha) = source_sha {
        fs::write(dir.join("source-sha"), format!("{sha}\n")).expect("write source-sha");
    }
    dir
}

fn templated_config() -> &'static str {
    "bot_member_id = \"${BOT_MEMBER_ID}\"\n"
}

fn templated_policy() -> &'static str {
    "permit(principal, action, resource == Conversation::\"channel:${ALLOWED_CHANNEL_ID}\");\n"
}

struct Run {
    output: Output,
    home: PathBuf,
}

impl Run {
    fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.output.stderr).into_owned()
    }

    fn installed(&self, relative: &str) -> PathBuf {
        self.home.join(".kiro/bots/kiro-help").join(relative)
    }
}

/// Run the entrypoint with a fake `git` and `kiro-bot` on PATH so it runs to the
/// real `exec` without needing a network or the actual binaries.
fn run_entrypoint(root: &Path, config_dir: &Path, extra_env: &[(&str, &str)], with_fakes: bool) -> Run {
    let home = root.join("home");
    let bin = root.join("bin");
    let worktree = root.join("worktree");
    fs::create_dir_all(&bin).expect("mkdir bin");

    let git_log = root.join("git.txt");
    let launch_record = root.join("launch.txt");
    let environment_record = root.join("environment.txt");

    if with_fakes {
        // Assert the exact git sequence, and fail loudly on anything unexpected
        // so a silent change in the clone protocol shows up as a test failure.
        write_exec(
            &bin.join("git"),
            r#"#!/bin/bash
set -euo pipefail
printf '%s\n' "$*" >> "$FAKE_GIT_LOG"
case "$*" in
  "-C $FAKE_GIT_WORKTREE init --quiet") ;;
  "-C $FAKE_GIT_WORKTREE remote remove origin") ;;
  "-C $FAKE_GIT_WORKTREE remote add origin $FAKE_GIT_URL") ;;
  "-C $FAKE_GIT_WORKTREE fetch --quiet --depth 1 origin $FAKE_SOURCE_SHA") ;;
  "-C $FAKE_GIT_WORKTREE checkout --quiet --detach FETCH_HEAD") ;;
  "-C $FAKE_GIT_WORKTREE rev-parse HEAD") printf '%s\n' "$FAKE_SOURCE_SHA" ;;
  *) echo "unexpected git command: $*" >&2; exit 2 ;;
esac
"#,
        );
        write_exec(
            &bin.join("kiro-bot"),
            "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$LAUNCH_RECORD\"\nenv > \"$ENVIRONMENT_RECORD\"\n",
        );
    }

    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
    let mut command = Command::new("bash");
    command
        .arg(entrypoint())
        .env_clear()
        .env("PATH", path)
        .env("HOME", &home)
        .env("KIRO_BOT_CONFIG_SOURCE_DIR", config_dir)
        .env("KIRO_BOT_WORKTREE_DIR", &worktree)
        .env("KIRO_BOT_GIT_URL", GIT_URL)
        .env("FAKE_GIT_LOG", &git_log)
        .env("FAKE_GIT_WORKTREE", &worktree)
        .env("FAKE_GIT_URL", GIT_URL)
        .env("FAKE_SOURCE_SHA", SOURCE_SHA)
        .env("LAUNCH_RECORD", &launch_record)
        .env("ENVIRONMENT_RECORD", &environment_record);
    for (key, value) in extra_env {
        command.env(key, value);
    }

    Run {
        output: command.output().expect("run entrypoint"),
        home,
    }
}

fn slack_env() -> Vec<(&'static str, &'static str)> {
    vec![
        ("BOT_MEMBER_ID", "UTEST12345"),
        ("ALLOWED_CHANNEL_ID", "CTEST12345"),
        ("SLACK_BOT_TOKEN", "bot-secret"),
        ("SLACK_APP_TOKEN", "app-secret"),
        ("KIRO_API_KEY", "api-secret"),
        ("GH_PAT", "github-secret"),
    ]
}

/// The privilege drop is the container's security boundary, and the ECS task
/// definition starts this container as root with RUNTIME_UID/GID set, so this
/// exercises that path for real rather than grepping the script: source order is
/// not execution order, and a grep passes even if the whole block is wrapped in
/// a condition that never fires.
///
/// Fakes `id` to report root so the branch is taken, and fakes `setpriv` to
/// record its argv instead of actually dropping. What must hold: no-new-privs,
/// cleared supplementary groups, the re-exec through tini, and that `/tmp` is
/// locked down BEFORE it is handed to the runtime uid — and that the script
/// execs setpriv rather than falling through to write config as root.
#[test]
fn drops_privileges_through_setpriv_and_tini_before_writing_config() {
    let root = tempfile::tempdir().expect("tempdir");
    let config = seed_config(root.path(), templated_config(), templated_policy(), Some(SOURCE_SHA));
    let bin = root.path().join("bin");
    fs::create_dir_all(&bin).expect("mkdir bin");

    // Report root so the drop branch is entered. `id -u`/`id -g` are also used
    // by the post-drop assertions, which this path never reaches.
    write_exec(&bin.join("id"), "#!/bin/sh\nprintf '0\\n'\n");
    // Record the argv and the order of the operations that precede it, then stop
    // — a real exec would re-enter the script as the unprivileged user.
    write_exec(
        &bin.join("setpriv"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$SETPRIV_RECORD\"\n",
    );
    write_exec(
        &bin.join("chmod"),
        "#!/bin/sh\nprintf 'chmod %s\\n' \"$*\" >> \"$ORDER_RECORD\"\nexit 0\n",
    );
    write_exec(
        &bin.join("chown"),
        "#!/bin/sh\nprintf 'chown %s\\n' \"$*\" >> \"$ORDER_RECORD\"\nexit 0\n",
    );

    let setpriv_record = root.path().join("setpriv.txt");
    let order_record = root.path().join("order.txt");
    let mut env = slack_env();
    env.push(("KIRO_BOT_RUNTIME_UID", "10001"));
    env.push(("KIRO_BOT_RUNTIME_GID", "10001"));
    let setpriv_path = setpriv_record.to_str().expect("utf-8 path");
    let order_path = order_record.to_str().expect("utf-8 path");
    env.push(("SETPRIV_RECORD", setpriv_path));
    env.push(("ORDER_RECORD", order_path));

    let run = run_entrypoint(root.path(), &config, &env, true);
    assert!(run.output.status.success(), "entrypoint failed: {}", run.stderr());

    let setpriv_argv = fs::read_to_string(&setpriv_record).expect("setpriv must be exec'd");
    for expected in [
        "--reuid=10001",
        "--regid=10001",
        "--clear-groups",
        "--no-new-privs",
        "/usr/bin/tini",
    ] {
        assert!(
            setpriv_argv.contains(expected),
            "setpriv argv missing {expected}: {setpriv_argv}"
        );
    }

    // /tmp must be private before it is handed to the runtime uid, or the
    // unprivileged process inherits a world-writable dir it then owns.
    let order = fs::read_to_string(&order_record).expect("order record");
    let chmod_tmp = order.find("chmod 0700 /tmp").expect("chmod 0700 /tmp");
    let chown = order.find("chown 10001:10001").expect("chown mounts");
    assert!(chmod_tmp < chown, "/tmp must be locked down before it changes owner");

    // The drop replaces the process, so nothing may have been generated yet.
    assert!(
        !run.installed("config.toml").exists(),
        "privileges must drop before any config is generated"
    );
    assert!(
        !run.installed("secrets.toml").exists(),
        "privileges must drop before secrets are written"
    );
}

/// The full happy path: substitute both identities, generate secrets.toml, clone
/// the worktree at the exact embedded sha, and exec the bot without leaking the
/// transport secrets into the child environment.
#[test]
fn starts_from_the_exact_source_sha_without_leaking_transport_secrets() {
    let root = tempfile::tempdir().expect("tempdir");
    let config = seed_config(root.path(), templated_config(), templated_policy(), Some(SOURCE_SHA));

    let run = run_entrypoint(root.path(), &config, &slack_env(), true);
    assert!(run.output.status.success(), "entrypoint failed: {}", run.stderr());

    // Substitution actually happened, in both files.
    let installed_config = fs::read_to_string(run.installed("config.toml")).expect("config.toml");
    assert!(installed_config.contains("UTEST12345"));
    let installed_policy = fs::read_to_string(run.installed("policies/agents.cedar")).expect("agents.cedar");
    assert!(installed_policy.contains("CTEST12345"));
    // secrets.toml is generated at runtime and must never be baked into the image.
    let installed_secrets = fs::read_to_string(run.installed("secrets.toml")).expect("secrets.toml");
    assert!(installed_secrets.contains("bot-secret"));

    let git_log = fs::read_to_string(root.path().join("git.txt")).expect("git log");
    let worktree = root.path().join("worktree").display().to_string();
    assert_eq!(git_log.trim().lines().collect::<Vec<_>>(), vec![
        format!("-C {worktree} init --quiet"),
        format!("-C {worktree} remote remove origin"),
        format!("-C {worktree} remote add origin {GIT_URL}"),
        format!("-C {worktree} fetch --quiet --depth 1 origin {SOURCE_SHA}"),
        format!("-C {worktree} checkout --quiet --detach FETCH_HEAD"),
        format!("-C {worktree} rev-parse HEAD"),
    ]);

    assert_eq!(
        fs::read_to_string(root.path().join("launch.txt"))
            .expect("launch record")
            .trim(),
        "start kiro-help --foreground"
    );

    // The git credential helper and the Slack transport tokens must not survive
    // into the bot process (and therefore into every MCP child it spawns).
    let child_env = fs::read_to_string(root.path().join("environment.txt")).expect("env record");
    assert!(child_env.contains("KIRO_API_KEY=api-secret"));
    for leaked in ["SLACK_BOT_TOKEN=", "SLACK_APP_TOKEN=", "GH_PAT=", "GIT_ASKPASS="] {
        assert!(!child_env.contains(leaked), "{leaked} leaked into the bot environment");
    }
}

/// Every value the script interpolates into generated TOML or Cedar is an
/// injection vector: the identities, and the Slack tokens that land in
/// secrets.toml. All of them must be rejected before anything is written, and the
/// rejection must not execute the payload.
#[test]
fn rejects_slack_values_that_could_alter_generated_configuration() {
    for (overrides, expected) in [
        (vec![("BOT_MEMBER_ID", "U|&$(touch injected)")], "invalid BOT_MEMBER_ID"),
        (
            vec![("ALLOWED_CHANNEL_ID", "C|&$(touch injected)")],
            "invalid ALLOWED_CHANNEL_ID",
        ),
        // A quote closes the TOML string and opens a second key; a newline does
        // the same on the next line. Both make the bot fail at parse time.
        (
            vec![("SLACK_BOT_TOKEN", "xoxb-1\", injected = \"1")],
            "invalid SLACK_BOT_TOKEN",
        ),
        (
            vec![("SLACK_APP_TOKEN", "xapp-1\ninjected = \"1\"")],
            "invalid SLACK_APP_TOKEN",
        ),
    ] {
        let root = tempfile::tempdir().expect("tempdir");
        let config = seed_config(root.path(), templated_config(), templated_policy(), Some(SOURCE_SHA));
        let marker = root.path().join("injected");

        let mut env = slack_env();
        env.extend(overrides.iter().copied());
        let run = run_entrypoint(root.path(), &config, &env, true);

        assert!(!run.output.status.success(), "must reject {overrides:?}");
        assert!(run.stderr().contains(expected), "stderr was: {}", run.stderr());
        assert!(!marker.exists(), "command substitution executed");
        assert!(
            !run.installed("secrets.toml").exists(),
            "no secrets may be written on rejection"
        );
    }
}

/// Fail closed if the upstream config files are not templates. This is not
/// hypothetical: the files on mainline once hardcoded a bot member id and two
/// invented channel ids, which would have produced a bot that answered in the
/// wrong channel rather than refusing to start.
#[test]
fn fails_closed_when_upstream_omits_a_slack_identity_placeholder() {
    for (config_body, policy_body, expected) in [
        (
            "bot_member_id = \"UUPSTREAM\"\n",
            templated_policy(),
            "config.toml missing BOT_MEMBER_ID placeholder",
        ),
        (
            templated_config(),
            "permit(principal, action, resource);\n",
            "agents.cedar missing ALLOWED_CHANNEL_ID placeholder",
        ),
    ] {
        let root = tempfile::tempdir().expect("tempdir");
        let config = seed_config(root.path(), config_body, policy_body, Some(SOURCE_SHA));

        let run = run_entrypoint(root.path(), &config, &slack_env(), true);

        assert!(!run.output.status.success(), "must refuse a non-template config");
        assert!(run.stderr().contains(expected), "stderr was: {}", run.stderr());
    }
}

/// The embedded sha pins what the container runs. A malformed one must fail at
/// startup rather than silently checking out whatever `git` resolves.
#[test]
fn rejects_a_malformed_embedded_source_sha() {
    for bad in ["not-a-sha", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", ""] {
        let root = tempfile::tempdir().expect("tempdir");
        let config = seed_config(root.path(), templated_config(), templated_policy(), Some(bad));

        let run = run_entrypoint(root.path(), &config, &slack_env(), true);

        assert!(!run.output.status.success(), "must reject source-sha {bad:?}");
        assert!(
            fs::read_to_string(root.path().join("git.txt")).is_err(),
            "must not touch git with an invalid sha"
        );
    }
}
