//! Export side: walk a KAS session directory, pack it into a zip, and
//! write the archive atomically to disk.

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{
    Path,
    PathBuf,
};

use uuid::Uuid;

use super::shared::SessionArchiveError;
use super::workspace_hash::compute_workspace_hash;

/// Options for [`export_session`].
pub struct ExportSessionOptions {
    /// Root sessions directory, e.g. `~/.kiro/sessions`.
    pub kas_sessions_root: PathBuf,
    /// Id of the session to export.
    pub session_id: String,
    /// Workspace paths under which the session is currently bucketed.
    /// The source directory is read from
    /// `{kas_sessions_root}/{compute_workspace_hash(workspace_paths)}/{session_id}/`.
    pub workspace_paths: Vec<String>,
    /// Destination archive path. If `out_path` has no extension,
    /// `.zip` is appended.
    pub out_path: PathBuf,
    /// If true, overwrite an existing file at the destination. If
    /// false, errors when the destination already exists.
    pub force: bool,
}

/// Pack a KAS session directory into a portable zip archive.
///
/// Reads from
/// `{kas_sessions_root}/{compute_workspace_hash(workspace_paths)}/{session_id}/`
/// and writes a zip to `out_path`. The archive is a focused capture
/// of the conversation: only `session.json` and `*.jsonl` files are
/// included, and `snapshots/` directories are skipped at every depth.
///
/// `snapshots/` holds content-hash-deduplicated file copies used for
/// checkpoint replay against the source workspace's filesystem state.
/// They are workspace-tied (replay is meaningless against a different
/// workspace), can be hundreds of MB on a long-running session, and
/// would expose unrelated source files if the archive is shared. The
/// import side already strips `lastCheckpointId`, so excluding the
/// underlying snapshots is consistent with that.
///
/// Steps:
///   1. Resolve source directory from the workspace hash.
///   2. Resolve final out_path (append `.zip` if no extension).
///   3. Validate destination (reject directories; reject existing files unless `force` is true).
///   4. Walk the source tree applying the include filter.
///   5. Write the zip atomically: the bytes go to a sibling temp file in the destination directory,
///      then rename onto `out_path`. A SIGKILL mid-write leaves the temp file (which is hidden by
///      its `.`-prefixed name) but never a truncated `out_path`.
///
/// Returns the absolute path of the written archive.
pub fn export_session(opts: ExportSessionOptions) -> Result<PathBuf, SessionArchiveError> {
    // 1. Locate source directory.
    let hash = compute_workspace_hash(&opts.workspace_paths);
    let session_dir = opts.kas_sessions_root.join(&hash).join(&opts.session_id);
    let session_meta = fs::metadata(&session_dir)
        .map_err(|_e| SessionArchiveError::msg(format!("session not found: {}", session_dir.display())))?;
    if !session_meta.is_dir() {
        return Err(SessionArchiveError::msg(format!(
            "session path is not a directory: {}",
            session_dir.display()
        )));
    }

    // 2. Resolve final out_path - append `.zip` only when no extension is present. If the user chose a
    //    different extension (e.g. `.bak`), respect their choice rather than mangling it.
    let final_path = if opts.out_path.extension().is_some() {
        opts.out_path.clone()
    } else {
        let mut p = opts.out_path.clone().into_os_string();
        p.push(".zip");
        PathBuf::from(p)
    };

    // 3. Validate destination.
    match fs::metadata(&final_path) {
        Ok(meta) if meta.is_dir() => {
            return Err(SessionArchiveError::msg(format!(
                "output path is a directory: {}",
                final_path.display()
            )));
        },
        Ok(_) if !opts.force => {
            return Err(SessionArchiveError::msg(format!(
                "output path already exists: {}",
                final_path.display()
            )));
        },
        Ok(_) | Err(_) => {
            // Either we're force-overwriting (atomic rename below replaces the existing file) or
            // the path is free.
        },
    }

    // 4. Walk source tree and assemble entries keyed by zip-internal path (POSIX separators, relative
    //    to session_dir).
    let mut entries: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    collect_files(&session_dir, "", &mut entries)?;

    // 5. Encode and write atomically: zip bytes go to a sibling temp file in the same directory, then
    //    rename onto `final_path`. A SIGKILL mid-write leaves the temp file (which is hidden by its
    //    `.`-prefixed name) but never a truncated `final_path`. Same-filesystem rename is atomic on
    //    POSIX and atomic-overwrite on Windows.
    let zip_bytes = encode_zip(&entries)?;
    let parent = final_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = final_path
        .file_name()
        .ok_or_else(|| SessionArchiveError::msg(format!("output path has no file name: {}", final_path.display())))?;
    let temp_path = parent.join(format!(
        ".{}.tmp.{}",
        file_name.to_string_lossy(),
        Uuid::new_v4().simple()
    ));
    fs::write(&temp_path, zip_bytes)
        .map_err(|e| SessionArchiveError::io(format!("write archive {}", temp_path.display()), e))?;
    if let Err(e) = fs::rename(&temp_path, &final_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(SessionArchiveError::io(
            format!("rename {} -> {}", temp_path.display(), final_path.display()),
            e,
        ));
    }

    Ok(final_path)
}

/// Recursively walk `dir`, adding `session.json` and `*.jsonl` files
/// to `entries`. `snapshots/` directories are skipped at every depth
/// (see [`export_session`] for rationale). Other regular files,
/// symlinks, and other non-file/non-dir entries are silently skipped.
fn collect_files(dir: &Path, prefix: &str, entries: &mut BTreeMap<String, Vec<u8>>) -> Result<(), SessionArchiveError> {
    let read = fs::read_dir(dir).map_err(|e| SessionArchiveError::io(format!("read dir {}", dir.display()), e))?;
    for dirent in read {
        let dirent = dirent.map_err(|e| SessionArchiveError::io(format!("iterate dir {}", dir.display()), e))?;
        let name = dirent.file_name();
        let name_str = name.to_string_lossy();
        let zip_key = if prefix.is_empty() {
            name_str.to_string()
        } else {
            format!("{prefix}/{name_str}")
        };
        let absolute = dir.join(&name);
        let file_type = dirent
            .file_type()
            .map_err(|e| SessionArchiveError::io(format!("stat {}", absolute.display()), e))?;
        if file_type.is_dir() {
            if name_str == "snapshots" {
                continue;
            }
            collect_files(&absolute, &zip_key, entries)?;
        } else if file_type.is_file() && is_included_file(&name_str) {
            let bytes =
                fs::read(&absolute).map_err(|e| SessionArchiveError::io(format!("read {}", absolute.display()), e))?;
            entries.insert(zip_key, bytes);
        }
    }
    Ok(())
}

/// Allowlist for export: `session.json` and any `*.jsonl` file. Lock
/// files (`session.json.lock`), partial writes (`*.tmp`), and any
/// future KAS-side files KAS adds are excluded by default until
/// explicitly added here.
fn is_included_file(name: &str) -> bool {
    name == "session.json" || name.ends_with(".jsonl")
}

/// Encode a name -> bytes map as a zip archive blob.
fn encode_zip(entries: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, SessionArchiveError> {
    let mut out = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut out));
        let opts = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries {
            writer
                .start_file(name.as_str(), opts)
                .map_err(|e| SessionArchiveError::io(format!("start zip entry {name}"), std::io::Error::other(e)))?;
            std::io::Write::write_all(&mut writer, bytes)
                .map_err(|e| SessionArchiveError::io(format!("write zip entry {name}"), e))?;
        }
        writer
            .finish()
            .map_err(|e| SessionArchiveError::io("finalize zip", std::io::Error::other(e)))?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{
        default_session_files,
        temp_base,
        write_kas_session,
    };
    use super::*;

    // -- export_session: happy path --

    #[test]
    fn export_writes_zip_at_out_path() {
        let dir = temp_base();
        let sessions_root = dir.path().join("sessions");
        let session_id = "my-session";
        let workspace = vec!["/work".to_string()];
        let hash = compute_workspace_hash(&workspace);
        write_kas_session(&sessions_root, &hash, session_id, &default_session_files(session_id));

        let out = dir.path().join("out.zip");
        let written = export_session(ExportSessionOptions {
            kas_sessions_root: sessions_root,
            session_id: session_id.to_string(),
            workspace_paths: workspace,
            out_path: out.clone(),
            force: false,
        })
        .unwrap();

        assert_eq!(written, out);
        let bytes = fs::read(&out).unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"));
    }

    #[test]
    fn export_appends_zip_extension_when_missing() {
        let dir = temp_base();
        let sessions_root = dir.path().join("sessions");
        let session_id = "s1";
        let workspace = vec!["/work".to_string()];
        write_kas_session(
            &sessions_root,
            &compute_workspace_hash(&workspace),
            session_id,
            &default_session_files(session_id),
        );

        let out = dir.path().join("archive-no-ext");
        let written = export_session(ExportSessionOptions {
            kas_sessions_root: sessions_root,
            session_id: session_id.to_string(),
            workspace_paths: workspace,
            out_path: out.clone(),
            force: false,
        })
        .unwrap();

        assert_eq!(
            written.extension().and_then(|s| s.to_str()),
            Some("zip"),
            "expected .zip extension, got {written:?}",
        );
        assert!(written.exists());
    }

    #[test]
    fn export_keeps_non_zip_extension() {
        // A user-chosen extension (e.g. `.bak`) is preserved; `.zip`
        // is only appended when no extension is present.
        let dir = temp_base();
        let sessions_root = dir.path().join("sessions");
        let session_id = "s1";
        let workspace = vec!["/work".to_string()];
        write_kas_session(
            &sessions_root,
            &compute_workspace_hash(&workspace),
            session_id,
            &default_session_files(session_id),
        );

        let out = dir.path().join("archive.kas");
        let written = export_session(ExportSessionOptions {
            kas_sessions_root: sessions_root,
            session_id: session_id.to_string(),
            workspace_paths: workspace,
            out_path: out.clone(),
            force: false,
        })
        .unwrap();
        assert_eq!(written, out);
    }

    #[test]
    fn export_force_overwrites_existing_file() {
        let dir = temp_base();
        let sessions_root = dir.path().join("sessions");
        let session_id = "s1";
        let workspace = vec!["/work".to_string()];
        write_kas_session(
            &sessions_root,
            &compute_workspace_hash(&workspace),
            session_id,
            &default_session_files(session_id),
        );

        let out = dir.path().join("archive.zip");
        fs::write(&out, b"old contents").unwrap();

        let written = export_session(ExportSessionOptions {
            kas_sessions_root: sessions_root,
            session_id: session_id.to_string(),
            workspace_paths: workspace,
            out_path: out.clone(),
            force: true,
        })
        .unwrap();

        let bytes = fs::read(&written).unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"), "force did not overwrite with zip");
    }

    #[test]
    fn export_leaves_no_temp_files_on_success() {
        // Atomicity invariant: after a successful export, the parent dir contains the final zip
        // and nothing else. The temp file used for atomic rename must be gone.
        let dir = temp_base();
        let sessions_root = dir.path().join("sessions");
        let session_id = "s1";
        let workspace = vec!["/work".to_string()];
        write_kas_session(
            &sessions_root,
            &compute_workspace_hash(&workspace),
            session_id,
            &default_session_files(session_id),
        );
        let out_dir = dir.path().join("out");
        fs::create_dir_all(&out_dir).unwrap();
        let out = out_dir.join("archive.zip");

        let written = export_session(ExportSessionOptions {
            kas_sessions_root: sessions_root,
            session_id: session_id.to_string(),
            workspace_paths: workspace,
            out_path: out,
            force: false,
        })
        .unwrap();

        let entries: Vec<_> = fs::read_dir(&out_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries, vec![written.file_name().unwrap().to_owned()]);
    }

    #[test]
    fn export_zip_includes_allowlisted_files_only() {
        let dir = temp_base();
        let sessions_root = dir.path().join("sessions");
        let session_id = "s1";
        let workspace = vec!["/work".to_string()];
        write_kas_session(
            &sessions_root,
            &compute_workspace_hash(&workspace),
            session_id,
            &default_session_files(session_id),
        );

        let out = dir.path().join("archive.zip");
        export_session(ExportSessionOptions {
            kas_sessions_root: sessions_root,
            session_id: session_id.to_string(),
            workspace_paths: workspace,
            out_path: out.clone(),
            force: false,
        })
        .unwrap();

        let bytes = fs::read(&out).unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"));
        // Cheap byte-search instead of re-decoding: confirm the
        // expected entry names appear and the excluded ones do not.
        let contains = |needle: &str| bytes.windows(needle.len()).any(|w| w == needle.as_bytes());
        for name in [
            "session.json",
            "messages.jsonl",
            "sub-executions/abc/session.json",
            "sub-executions/abc/messages.jsonl",
        ] {
            assert!(contains(name), "expected zip to contain {name}");
        }
        for name in [
            "snapshots/deadbeef",
            "sub-executions/abc/snapshots/cafe",
            "session.json.lock",
        ] {
            assert!(!contains(name), "expected zip to exclude {name}");
        }
    }

    // -- export_session: error paths --

    #[test]
    fn export_errors_when_session_missing() {
        let dir = temp_base();
        let sessions_root = dir.path().join("sessions");
        let err = export_session(ExportSessionOptions {
            kas_sessions_root: sessions_root,
            session_id: "does-not-exist".to_string(),
            workspace_paths: vec!["/work".to_string()],
            out_path: dir.path().join("archive.zip"),
            force: false,
        })
        .unwrap_err();
        assert!(
            err.to_string().contains("session not found"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn export_errors_when_destination_is_directory() {
        let dir = temp_base();
        let sessions_root = dir.path().join("sessions");
        let session_id = "s1";
        let workspace = vec!["/work".to_string()];
        write_kas_session(
            &sessions_root,
            &compute_workspace_hash(&workspace),
            session_id,
            &default_session_files(session_id),
        );

        let out = dir.path().join("output-dir.zip");
        fs::create_dir_all(&out).unwrap();

        let err = export_session(ExportSessionOptions {
            kas_sessions_root: sessions_root,
            session_id: session_id.to_string(),
            workspace_paths: workspace,
            out_path: out,
            force: true,
        })
        .unwrap_err();
        assert!(
            err.to_string().contains("output path is a directory"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn export_errors_when_destination_exists_without_force() {
        let dir = temp_base();
        let sessions_root = dir.path().join("sessions");
        let session_id = "s1";
        let workspace = vec!["/work".to_string()];
        write_kas_session(
            &sessions_root,
            &compute_workspace_hash(&workspace),
            session_id,
            &default_session_files(session_id),
        );

        let out = dir.path().join("archive.zip");
        fs::write(&out, b"existing").unwrap();

        let err = export_session(ExportSessionOptions {
            kas_sessions_root: sessions_root,
            session_id: session_id.to_string(),
            workspace_paths: workspace,
            out_path: out,
            force: false,
        })
        .unwrap_err();
        assert!(
            err.to_string().contains("output path already exists"),
            "unexpected message: {err}"
        );
    }
}
