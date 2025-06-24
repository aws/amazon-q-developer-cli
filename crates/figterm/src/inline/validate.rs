use std::path::Path;

pub(super) fn validate(command: &str) -> bool {
    validate_with_context(
        command,
        || {
            std::env::current_dir()
                .ok()
                .and_then(|s| s.to_str().map(ToOwned::to_owned))
        },
        || fig_util::directories::home_dir_utf8().ok(),
        |key| std::env::var(key).map(Some),
    )
}

fn validate_with_context<CwdStr, Cwd, HdStr, Hd, CtxStr, Ctx, E>(
    command: &str,
    cwd: Cwd,
    home_dir: Hd,
    ctx: Ctx,
) -> bool
where
    CwdStr: AsRef<str>,
    Cwd: FnOnce() -> Option<CwdStr>,
    HdStr: AsRef<str>,
    Hd: FnOnce() -> Option<HdStr>,
    CtxStr: AsRef<str>,
    Ctx: FnMut(&str) -> Result<Option<CtxStr>, E>,
{
    let command = command.trim();

    // Currently the api responds with redactions of `XXX` if there is PII identified, just filter
    // these to improve quality
    if command.contains("XXX") {
        return false;
    }

    // Try to validate the args to "cd",
    if let Some(args) = shlex::split(command) {
        if args.first().map(|s| s.as_str()) == Some("cd") && args.len() == 2 {
            if let Some(arg) = args.get(1) {
                if let Ok(arg) = shellexpand::full_with_context(arg, home_dir, ctx) {
                    let path = Path::new(arg.as_ref());
                    if path.is_absolute() && !path.is_dir() {
                        return false;
                    }
                    let canonicalized = cwd().and_then(|cwd| Path::new(cwd.as_ref()).join(path).canonicalize().ok());
                    match canonicalized {
                        Some(p) if !p.is_dir() => return false,
                        Some(_) => {},
                        None => return false,
                    }
                }
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate() {
        let tempdir = tempfile::tempdir().unwrap();

        // Create structure to test with
        std::fs::create_dir_all(tempdir.path().join("a")).unwrap();
        std::fs::create_dir_all(tempdir.path().join("a").join("b")).unwrap();
        std::fs::create_dir_all(tempdir.path().join("space in name")).unwrap();

        std::fs::write(tempdir.path().join("file"), "hello").unwrap();
        std::fs::write(tempdir.path().join("a").join("file"), "hello").unwrap();

        let tempdir_path = tempdir.path().to_str().unwrap();
        let cwd = || -> Option<&str> { Some(tempdir_path) };
        let home_dir = || -> Option<&str> { Some(tempdir_path) };
        let context = |key: &str| -> anyhow::Result<Option<&str>> {
            match key {
                "HOME" => anyhow::Ok(Some(tempdir_path)),
                "A" => anyhow::Ok(Some("a")),
                "B" => anyhow::Ok(Some("b")),
                _ => Ok(None),
            }
        };
        let valid = |response: &str| validate_with_context(response, cwd, home_dir, context);

        // Allows normal commands
        assert!(valid(r#"echo "hello""#));
        assert!(valid(r#"aws s3 cp file s3://bucket"#));

        // Rejects redactions
        assert!(!valid(r#"git clone "XXXXXXXXXXXXXXXXXXXXXXXXXX""#));
        assert!(!valid(r#"echo "XXX""#));
        assert!(!valid(r#"curl -X POST "XXXXXXXXXXXXXXXXXXXXXXXXXX""#));

        // Allows cd commands with folders that eixts
        assert!(valid(r#"cd /tmp"#));
        assert!(valid(r#"cd "/tmp""#));
        assert!(valid(r#"cd ~"#));
        assert!(valid(r#"cd ~/"#));
        assert!(valid(r#"cd ~/a"#));
        assert!(valid(r#"cd ~/a/"#));
        assert!(valid(r#"cd ~/a/b"#));
        assert!(valid(r#"cd ~/$A"#));
        assert!(valid(r#"cd ~/$A/$B"#));
        assert!(valid(r#"cd ~/space\ in\ name"#));
        assert!(valid(r#"cd ~/"space in name""#));
        assert!(valid(r#"cd a"#));
        assert!(valid(r#"cd a/"#));
        assert!(valid(r#"cd a/b"#));
        assert!(valid(r#"cd $A"#));
        assert!(valid(r#"cd $A/$B"#));
        assert!(valid(r#"cd space\ in\ name"#));
        assert!(valid(r#"cd "space in name""#));

        // Rejects cd commands that don't exist or are not folders
        assert!(!valid(r#"cd /folder/doesnt/exist"#));
        assert!(!valid(r#"cd /file"#));
        assert!(!valid(r#"cd ~/file"#));
        assert!(!valid(r#"cd ~/a/file"#));
        assert!(!valid(r#"cd ~/$A/file"#));
        assert!(!valid(r#"cd ~/$A/$B/file"#));
        assert!(!valid(r#"cd b"#));
        assert!(!valid(r#"cd b/"#));
        assert!(!valid(r#"cd file"#));
        assert!(!valid(r#"cd $B"#));
        assert!(!valid(r#"cd a/b/file"#));

        // Cases that are not currently rejected due to ambiguity
        assert!(valid(r#"cd space in name"#));
    }
}
