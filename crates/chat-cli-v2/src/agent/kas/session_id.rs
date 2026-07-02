//! Generators for KAS session ids stamped onto converter / importer
//! output.
//!
//! Single source of truth so V2 -> KAS conversion (in
//! [`super::v2_to_kas`]) and KAS-zip import (in [`super::import`])
//! produce ids in a consistent shape.
//!
//! # Format
//!
//! - With a source id: `cli_{source_id}_{8 random alphanumeric}`. The prefix encodes provenance
//!   ("this came from a converted session whose source id was X"); the random suffix is the
//!   collision-prevention mechanism that lets the same source be converted any number of times,
//!   each landing in its own target session.
//! - Without a source id: `cli_{uuid v4}`. Used when the input has no meaningful upstream id -
//!   notably, KAS-zip imports where the archive's own session id is not preserved.
//!
//! KAS surfaces tolerate arbitrary id strings; the `cli_` prefix is
//! observability sugar, not a requirement.

use rand::RngExt;
use rand::distr::Alphanumeric;

/// Length of the random suffix appended after the source id. Eight
/// alphanumeric characters give 62^8 ~= 2.18 * 10^14 distinct values
/// per source id, large enough that re-conversion-after-deletion is
/// safe under any realistic load.
const SUFFIX_LEN: usize = 8;

/// Generate a KAS session id for a converted or imported session.
///
/// `source` is the id of the session in its origin engine, when one
/// exists. V2 -> KAS conversion passes `Some(v2_session_id)` so the
/// new id encodes provenance. Each call mints a fresh random suffix,
/// so two conversions of the same V2 session produce two distinct
/// KAS ids - cross-engine conversion is not deduped at the id level.
/// KAS-zip import passes `None` because the archive's own session id
/// is not preserved across the import boundary; a UUID is used.
pub fn generate_kas_session_id(source: Option<&str>) -> String {
    match source {
        Some(s) => format!("cli_{s}_{}", random_suffix()),
        None => format!("cli_{}", uuid::Uuid::new_v4()),
    }
}

/// Sample [`SUFFIX_LEN`] alphanumeric characters from the OS RNG.
fn random_suffix() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(SUFFIX_LEN)
        .map(char::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Some(source)` produces a `cli_{source}_{suffix}` id with a
    /// non-empty random suffix.
    #[test]
    fn with_source_returns_prefixed_id_with_suffix() {
        let v2_id = "8e247382-c779-440f-b1db-ed9bad1afaee";
        let id = generate_kas_session_id(Some(v2_id));
        let prefix = format!("cli_{v2_id}_");
        assert!(id.starts_with(&prefix), "got id: {id}");
        let suffix = id.strip_prefix(&prefix).expect("prefix matches");
        assert_eq!(suffix.len(), SUFFIX_LEN);
        assert!(
            suffix.chars().all(|c| c.is_ascii_alphanumeric()),
            "suffix not alphanumeric: {suffix:?}"
        );
    }

    /// Two calls with the same source return distinct ids - the
    /// random suffix is the collision-prevention mechanism for
    /// re-conversion of the same source.
    #[test]
    fn with_source_is_unique_across_calls() {
        let v2_id = "abc-123";
        let a = generate_kas_session_id(Some(v2_id));
        let b = generate_kas_session_id(Some(v2_id));
        assert_ne!(a, b, "two calls with the same source produced the same id");
    }

    /// `None` produces a `cli_{uuid}` id - no source id is encoded
    /// because there isn't one to encode.
    #[test]
    fn without_source_returns_cli_uuid() {
        let id = generate_kas_session_id(None);
        let suffix = id.strip_prefix("cli_").expect("cli_ prefix");
        uuid::Uuid::parse_str(suffix).expect("suffix is a valid uuid");
    }

    /// Two `None` calls produce distinct ids (uuid v4 collision is
    /// not a real concern; this just pins the property).
    #[test]
    fn without_source_is_unique_across_calls() {
        let a = generate_kas_session_id(None);
        let b = generate_kas_session_id(None);
        assert_ne!(a, b);
    }

    /// `Some(source)` produces ids whose directory name starts with
    /// `cli_{source}_`. This prefix invariant is what readers (e.g.
    /// listing UIs surfacing provenance) rely on to identify
    /// converted sessions originating from a given source id.
    #[test]
    fn with_source_format_carries_source_id_prefix() {
        let v2_id = "8e247382-c779-440f-b1db-ed9bad1afaee";
        let id = generate_kas_session_id(Some(v2_id));
        let expected_prefix = format!("cli_{v2_id}_");
        assert!(id.starts_with(&expected_prefix));
    }
}
