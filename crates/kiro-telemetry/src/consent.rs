use std::fs;
use std::path::Path;

use crate::MetricRecord;
use crate::metric::{
    self,
    ConsentCheckKind,
    ConsentIntegrityResult,
};

pub fn consent_record_integrity_record(check_kind: ConsentCheckKind, result: ConsentIntegrityResult) -> MetricRecord {
    metric::consent_record_integrity(check_kind, result)
}

pub fn consent_file_integrity_records(path: impl AsRef<Path>) -> Vec<MetricRecord> {
    let path = path.as_ref();
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return records_for_all(ConsentIntegrityResult::Missing);
        },
        Err(_) => return records_for_all(ConsentIntegrityResult::Unreadable),
    };

    if metadata.file_type().is_symlink() {
        return records_for_all(ConsentIntegrityResult::Tampered);
    }

    vec![
        consent_record_integrity_record(ConsentCheckKind::Hash, hash_result(path)),
        consent_record_integrity_record(ConsentCheckKind::Perms, permissions_result(&metadata)),
        consent_record_integrity_record(ConsentCheckKind::Owner, owner_result(&metadata)),
    ]
}

fn records_for_all(result: ConsentIntegrityResult) -> Vec<MetricRecord> {
    [ConsentCheckKind::Hash, ConsentCheckKind::Perms, ConsentCheckKind::Owner]
        .into_iter()
        .map(|check_kind| consent_record_integrity_record(check_kind, result))
        .collect()
}

fn hash_result(path: &Path) -> ConsentIntegrityResult {
    match fs::read(path) {
        Ok(bytes) if serde_json::from_slice::<serde_json::Value>(&bytes).is_ok() => ConsentIntegrityResult::Ok,
        Ok(_) => ConsentIntegrityResult::Tampered,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ConsentIntegrityResult::Missing,
        Err(_) => ConsentIntegrityResult::Unreadable,
    }
}

#[cfg(unix)]
fn permissions_result(metadata: &fs::Metadata) -> ConsentIntegrityResult {
    use std::os::unix::fs::PermissionsExt;

    if metadata.permissions().mode() & 0o077 == 0 {
        ConsentIntegrityResult::Ok
    } else {
        ConsentIntegrityResult::Tampered
    }
}

#[cfg(not(unix))]
fn permissions_result(_metadata: &fs::Metadata) -> ConsentIntegrityResult {
    ConsentIntegrityResult::Ok
}

#[cfg(unix)]
fn owner_result(metadata: &fs::Metadata) -> ConsentIntegrityResult {
    use std::os::unix::fs::MetadataExt;

    // SAFETY: geteuid has no preconditions and does not dereference pointers.
    let effective_uid = unsafe { libc::geteuid() };
    if metadata.uid() == effective_uid {
        ConsentIntegrityResult::Ok
    } else {
        ConsentIntegrityResult::Tampered
    }
}

#[cfg(not(unix))]
fn owner_result(_metadata: &fs::Metadata) -> ConsentIntegrityResult {
    ConsentIntegrityResult::Ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::metric_attr;

    #[test]
    fn builds_schema_backed_consent_integrity_record() {
        let record = consent_record_integrity_record(ConsentCheckKind::Perms, ConsentIntegrityResult::Ok);

        assert_eq!(record.name, "kiro_cli_consent_record_integrity_total");
        assert_eq!(metric_attr(&record, "check_kind"), Some("perms"));
        assert_eq!(metric_attr(&record, "integrity_result"), Some("ok"));
    }

    #[test]
    fn missing_consent_file_reports_missing_for_all_local_checks() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let records = consent_file_integrity_records(tempdir.path().join("missing.json"));

        assert_eq!(records.len(), 3);
        assert!(
            records
                .iter()
                .all(|record| metric_attr(record, "integrity_result") == Some("missing"))
        );
    }

    #[test]
    fn malformed_consent_file_reports_hash_tampering() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let path = tempdir.path().join("settings.json");
        fs::write(&path, b"{not-json").expect("write");

        let records = consent_file_integrity_records(path);

        assert!(records.iter().any(|record| {
            metric_attr(record, "check_kind") == Some("hash")
                && metric_attr(record, "integrity_result") == Some("tampered")
        }));
    }

    #[test]
    fn secure_consent_file_reports_ok() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let path = tempdir.path().join("settings.json");
        fs::write(&path, b"{}").expect("write");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("chmod");
        }

        let records = consent_file_integrity_records(path);

        assert!(
            records
                .iter()
                .all(|record| metric_attr(record, "integrity_result") == Some("ok"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn world_readable_consent_file_reports_tampered_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let tempdir = tempfile::tempdir().expect("tempdir");
        let path = tempdir.path().join("settings.json");
        fs::write(&path, b"{}").expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");

        let records = consent_file_integrity_records(path);

        assert!(records.iter().any(|record| {
            metric_attr(record, "check_kind") == Some("perms")
                && metric_attr(record, "integrity_result") == Some("tampered")
        }));
    }
}
