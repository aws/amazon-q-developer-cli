use std::time::Duration;

use serde::{
    Deserialize,
    Serialize,
};
use tracing::warn;

use crate::util::consts::env_var::{
    KIRO_BUNDLED_FEED_FILE,
    KIRO_FEED_URL,
    KIRO_NO_REMOTE_CHANGELOG,
};

/// The changelog feed compiled into the binary; the guaranteed fallback when
/// no remote feed or cache is available.
const EMBEDDED_FEED: &str = include_str!("./feed.json");

const GAMMA_FEED_URL: &str = "https://download.gamma.cli.kiro.dev/stable/changelog/feed.json";
const PROD_FEED_URL: &str = "https://prod.download.cli.kiro.dev/stable/changelog/feed.json";

/// Total request budget for fetching the remote feed. Kept short because any
/// failure falls back to the cached/bundled feed.
const FETCH_TIMEOUT: Duration = Duration::from_secs(3);

/// Upper bound on the remote feed size, comfortably above any retained feed
/// (the largest published so far is ~100KB).
const MAX_FEED_SIZE: u64 = 2 * 1024 * 1024;

/// Atomically replaces `path` with `contents` via a uniquely named temp file
/// in the same directory + rename, so concurrent writers (background refresh,
/// /changelog, parallel launches) can never leave a torn file for readers.
pub fn atomic_write(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("path has no parent directory"))?;
    std::fs::create_dir_all(parent)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut tmp, contents.as_bytes())?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Feed URLs must be https, except plain http to the local loopback (tests).
/// Exact host comparison, so e.g. `http://localhost.evil.com` is rejected.
fn is_allowed_feed_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    match parsed.scheme() {
        "https" => true,
        "http" => matches!(parsed.host_str(), Some("127.0.0.1" | "localhost")),
        _ => false,
    }
}

/// Monotonic freshness key for a feed: the highest semver among release
/// entries (hidden included, since placeholders still order feeds), capped at
/// `at_most` when given so both sides of a freshness comparison are measured
/// under the same cap. `None` orders below any `Some`.
fn max_release_version(feed: &Feed, at_most: Option<&semver::Version>) -> Option<semver::Version> {
    feed.entries
        .iter()
        .filter(|entry| entry.entry_type == "release")
        .filter_map(|entry| semver::Version::parse(&entry.version).ok())
        .filter(|version| at_most.is_none_or(|cap| version <= cap))
        .max()
}

/// Strips control characters (including ANSI/OSC escape introducers) from all
/// string values in remotely fetched JSON, since feed content is rendered
/// directly to the user's terminal. Newlines and tabs are kept.
fn sanitize_json_strings(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) => {
            if s.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\t')) {
                *s = s
                    .chars()
                    .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
                    .collect();
            }
        },
        serde_json::Value::Array(values) => values.iter_mut().for_each(sanitize_json_strings),
        serde_json::Value::Object(map) => map.values_mut().for_each(sanitize_json_strings),
        _ => {},
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Feed {
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub date: String,
    pub version: String,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    #[serde(rename = "type")]
    pub change_type: String,
    pub description: String,
}

impl Feed {
    pub fn load() -> Self {
        serde_json::from_str(&Self::bundled_json())
            .unwrap_or_else(|_| serde_json::from_str(EMBEDDED_FEED).expect("embedded feed.json is valid json"))
    }

    /// Raw bundled feed JSON. Honors `KIRO_BUNDLED_FEED_FILE` (a test/dev
    /// seam) so the version cap and embedded floor can be exercised against a
    /// fixture instead of the compile-time feed; falls back to the embedded
    /// copy when the override is unset or unreadable, so the guaranteed
    /// floor is never lost in normal operation.
    fn bundled_json() -> std::borrow::Cow<'static, str> {
        if let Some(path) = std::env::var_os(KIRO_BUNDLED_FEED_FILE)
            && !path.is_empty()
        {
            match std::fs::read_to_string(&path) {
                Ok(contents) => return std::borrow::Cow::Owned(contents),
                Err(err) => warn!(?path, %err, "failed to read KIRO_BUNDLED_FEED_FILE; using embedded feed"),
            }
        }
        std::borrow::Cow::Borrowed(EMBEDDED_FEED)
    }

    /// URL of the remotely published changelog feed, if this build should
    /// read one. Nightly, rc, and feature builds read the gamma distribution
    /// (all three ship to gamma only); stable builds read prod only when the
    /// `remote_changelog` rollout gate is on, so the stable ramp is
    /// controlled centrally. Local dev builds never fetch.
    /// KIRO_FEED_URL redirects the fetch but does not bypass the channel or
    /// rollout gate; use KIRO_VERSION_OVERRIDE to test channel-gated behavior.
    pub fn remote_url() -> Option<String> {
        Self::resolve_remote_url(
            std::env::var_os(KIRO_NO_REMOTE_CHANGELOG).is_some_and(|v| !v.is_empty()),
            &crate::util::channel::channel(),
            crate::rollout::rollout().is_enabled(crate::rollout::Feature::RemoteChangelog),
            std::env::var(KIRO_FEED_URL)
                .ok()
                .filter(|url| !url.is_empty())
                .as_deref(),
        )
    }

    /// Pure resolution of the feed URL from its inputs, so the full matrix
    /// (channel x gate x override x kill switch) is testable without touching
    /// process env or the global rollout.
    fn resolve_remote_url(
        kill_switch: bool,
        channel: &crate::util::channel::Channel,
        stable_gate_enabled: bool,
        url_override: Option<&str>,
    ) -> Option<String> {
        // Kill switch: turn off remote fetching without a binary release.
        if kill_switch {
            return None;
        }
        let cdn_url = match channel {
            // Nightly, rc, and feature builds all ship to gamma only, so they
            // read the gamma feed. Version capping keeps a pre-release build
            // from advertising the release it precedes.
            crate::util::channel::Channel::Nightly
            | crate::util::channel::Channel::Rc
            | crate::util::channel::Channel::Other(_) => GAMMA_FEED_URL,
            // Stable remote fetch is gated behind the rollout system so it can
            // be ramped (internal -> percentage -> 100%).
            crate::util::channel::Channel::Stable if stable_gate_enabled => PROD_FEED_URL,
            // Local dev builds stay hermetic: no network on launch or
            // /changelog. KIRO_FEED_URL cannot override this -- it only
            // redirects fetches the channel/gate already authorized.
            crate::util::channel::Channel::Stable | crate::util::channel::Channel::Dev => return None,
        };
        match url_override {
            // Feed content is rendered to the user's terminal, so never allow
            // it over plaintext transport (loopback excepted, for tests).
            Some(url) if is_allowed_feed_url(url) => Some(url.to_string()),
            Some(url) => {
                warn!(%url, "ignoring non-https KIRO_FEED_URL");
                None
            },
            None => Some(cdn_url.to_string()),
        }
    }

    /// Fetches the published feed for builds with a remote feed URL, caching
    /// the raw body + ETag on success and returning the validated, sanitized,
    /// version-capped JSON. Uses a conditional GET so an unchanged feed costs
    /// a 304 with no body (served from the cache). Returns `None` when this
    /// build does not fetch remotely and on any network/parse failure, so
    /// callers fall back to the cached or bundled feed.
    pub async fn fetch_remote_json() -> Option<String> {
        let url = Self::remote_url()?;
        let client = crate::request::new_client_no_redirects().ok()?;
        let cached_raw = Self::read_cache_raw_for(&url);
        let mut request = client.get(&url).timeout(FETCH_TIMEOUT);
        // Only revalidate an ETag whose cached body is still present.
        if cached_raw.is_some()
            && let Some(etag) = Self::read_cache_etag()
        {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        let mut response = match request.send().await {
            Ok(response) => response,
            Err(err) => {
                warn!(%url, %err, "failed to fetch remote changelog feed");
                return None;
            },
        };
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            let processed = cached_raw
                .and_then(|raw| Self::postprocess_remote_feed(&raw, &crate::util::channel::cli_version_string()));
            if processed.is_none() {
                // A 304 whose cached body is unusable would otherwise never
                // self-heal (the stale ETag keeps yielding 304s); drop the
                // pair so the next fetch is unconditional.
                Self::remove_cache();
            }
            return processed;
        }
        if !response.status().is_success() {
            warn!(%url, status = %response.status(), "remote changelog feed returned non-success status");
            return None;
        }
        if response.content_length().is_some_and(|len| len > MAX_FEED_SIZE) {
            warn!(%url, "remote changelog feed exceeds size limit");
            return None;
        }
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        // Stream the body so the size cap bounds memory, not just the final
        // buffer.
        let mut body = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    if body.len() + chunk.len() > MAX_FEED_SIZE as usize {
                        warn!(%url, "remote changelog feed exceeds size limit");
                        return None;
                    }
                    body.extend_from_slice(&chunk);
                },
                Ok(None) => break,
                Err(err) => {
                    warn!(%url, %err, "failed reading remote changelog feed body");
                    return None;
                },
            }
        }
        let body = String::from_utf8(body).ok()?;
        let Some(processed) = Self::postprocess_remote_feed(&body, &crate::util::channel::cli_version_string()) else {
            warn!(%url, "remote changelog feed is not valid feed JSON");
            return None;
        };
        // Cache the raw body, not the processed output: processing (sanitize
        // + version cap) is re-applied on every read, so a cache written by
        // one binary version is never served with another version's cap, and
        // the ETag always corresponds to the server's representation.
        Self::write_cache(&body, etag.as_deref(), &url);
        Some(processed)
    }

    /// Validates, sanitizes, and version-caps a fetched feed body, returning
    /// the JSON to serve to consumers or `None` if it is not a valid feed.
    fn postprocess_remote_feed(body: &str, current_version: &str) -> Option<String> {
        let mut value = serde_json::from_str::<serde_json::Value>(body).ok()?;
        sanitize_json_strings(&mut value);
        Feed::deserialize(&value).ok()?;
        Self::drop_entries_newer_than(&mut value, current_version);
        Some(value.to_string())
    }

    /// Removes feed entries for versions newer than the running binary, so a
    /// remotely published feed (which may already list releases the user does
    /// not have) never advertises unreleased changes. Operates on the raw JSON
    /// to preserve fields the [Feed] struct does not model (e.g. `title`,
    /// read by the TUI). Entries with unparseable versions are kept so
    /// consumers can apply their own matching (e.g. `2.X.X` wildcards).
    fn drop_entries_newer_than(value: &mut serde_json::Value, current_version: &str) {
        let Ok(current) = semver::Version::parse(current_version) else {
            return;
        };
        if let Some(entries) = value.get_mut("entries").and_then(|entries| entries.as_array_mut()) {
            entries.retain(|entry| {
                entry
                    .get("version")
                    .and_then(|version| version.as_str())
                    .and_then(|version| semver::Version::parse(version).ok())
                    .is_none_or(|version| version <= current)
            });
        }
    }

    /// Loads the feed remote-first (the fetch updates the on-disk cache).
    /// Falls back to the cached copy, then the bundled feed. Blocks on the
    /// network (bounded by [FETCH_TIMEOUT]); startup paths should use
    /// [Self::load_cached] + [Self::refresh_cache_in_background] instead.
    pub async fn load_remote() -> Self {
        match Self::fetch_remote_json().await {
            Some(json) => serde_json::from_str(&json).unwrap_or_else(|_| Self::load()),
            None => Self::load_cached(),
        }
    }

    /// Loads the feed from the on-disk cache written by a previous remote
    /// fetch, without touching the network. Entries are re-capped at the
    /// running binary's version in case the cache was written by a newer
    /// binary. Builds that do not fetch remotely, and cache misses, get the
    /// bundled feed.
    pub fn load_cached() -> Self {
        serde_json::from_str(&Self::load_cached_json()).unwrap_or_else(|_| Self::load())
    }

    /// Raw-JSON variant of [Self::load_cached], preserving fields the [Feed]
    /// struct does not model (e.g. `title`, read by the TUI).
    pub fn load_cached_json() -> String {
        let bundled = Self::bundled_json();
        let cached = || {
            let url = Self::remote_url()?;
            let raw = Self::read_cache_raw_for(&url)?;
            let processed = Self::postprocess_remote_feed(&raw, &crate::util::channel::cli_version_string())?;
            // Embedded floor: after a binary upgrade the freshly bundled feed
            // can be newer than a stale cache from the previous binary; never
            // serve the older of the two. Both sides are measured under the
            // current binary's version cap so an ahead-of-release bundled
            // entry cannot unfairly outrank a correctly capped cache.
            let cap = crate::util::channel::cli_version();
            let cached_feed: Feed = serde_json::from_str(&processed).ok()?;
            (max_release_version(&cached_feed, cap.as_ref()) >= max_release_version(&Self::load(), cap.as_ref()))
                .then_some(processed)
        };
        cached().unwrap_or_else(|| bundled.into_owned())
    }

    /// Spawns a background task refreshing the on-disk cache from the remote
    /// feed. No-op when this build does not fetch remotely or outside a tokio
    /// runtime. Never blocks the caller.
    pub fn refresh_cache_in_background() {
        if Self::remote_url().is_none() {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        handle.spawn(async {
            // The fetch itself writes the cache on success. Surface any
            // unexpected panic, which a detached task would otherwise swallow.
            let fetch = std::panic::AssertUnwindSafe(async {
                let _ = Self::fetch_remote_json().await;
            });
            if futures::FutureExt::catch_unwind(fetch).await.is_err() {
                warn!("background changelog feed refresh panicked");
            }
        });
    }

    /// Cached body, but only when it was fetched from `url`. The cache lives in
    /// a shared data dir, so a channel switch (gamma <-> prod) must not serve
    /// another distribution's content.
    fn read_cache_raw_for(url: &str) -> Option<String> {
        if Self::read_cache_source().as_deref() != Some(url) {
            return None;
        }
        let path = crate::util::paths::feed_cache_json_path().ok()?;
        std::fs::read_to_string(path).ok()
    }

    fn read_cache_source() -> Option<String> {
        let path = crate::util::paths::feed_cache_source_path().ok()?;
        let source = std::fs::read_to_string(path).ok()?;
        let source = source.trim();
        (!source.is_empty()).then(|| source.to_string())
    }

    fn read_cache_etag() -> Option<String> {
        let path = crate::util::paths::feed_cache_etag_path().ok()?;
        let etag = std::fs::read_to_string(path).ok()?;
        let etag = etag.trim();
        (!etag.is_empty()).then(|| etag.to_string())
    }

    fn remove_cache() {
        for path in [
            crate::util::paths::feed_cache_json_path(),
            crate::util::paths::feed_cache_etag_path(),
            crate::util::paths::feed_cache_source_path(),
        ]
        .into_iter()
        .flatten()
        {
            let _ = std::fs::remove_file(path);
        }
    }

    /// Best-effort atomic write of the feed cache and its ETag (unique temp
    /// file + rename, so concurrent writers cannot tear each other's writes).
    fn write_cache(raw_json: &str, etag: Option<&str>, source_url: &str) {
        let Ok(path) = crate::util::paths::feed_cache_json_path() else {
            return;
        };
        if atomic_write(&path, raw_json).is_err() {
            return;
        }
        // Record the source before the ETag: a body with no recorded source is
        // treated as a miss, which is the safe direction.
        if let Ok(source_path) = crate::util::paths::feed_cache_source_path() {
            let _ = atomic_write(&source_path, source_url);
        }
        if let Ok(etag_path) = crate::util::paths::feed_cache_etag_path() {
            match etag {
                Some(etag) => {
                    let _ = atomic_write(&etag_path, etag);
                },
                None => {
                    let _ = std::fs::remove_file(&etag_path);
                },
            }
        }
    }

    pub fn get_version_changelog(&self, version: &str) -> Option<Entry> {
        self.entries
            .iter()
            .find(|entry| entry.entry_type == "release" && entry.version == version && !entry.hidden)
            .cloned()
    }

    pub fn get_all_changelogs(&self) -> Vec<Entry> {
        self.entries
            .iter()
            .filter(|entry| entry.entry_type == "release" && !entry.hidden)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::consts::env_var::KIRO_VERSION_OVERRIDE;

    fn sample_feed_json() -> &'static str {
        r#"{
            "entries": [
                {
                    "type": "release",
                    "date": "2026-01-01",
                    "version": "0.1.0",
                    "changes": [{ "type": "added", "description": "Remote entry" }]
                }
            ]
        }"#
    }

    /// Full URL-resolution matrix without touching process env or the global
    /// rollout: channel x stable gate x override x kill switch.
    #[test]
    fn test_resolve_remote_url_matrix() {
        use crate::util::channel::Channel;

        let resolve = |kill, ch: &Channel, gate, over| Feed::resolve_remote_url(kill, ch, gate, over);
        let feature = Channel::Other("fix-foo".to_string());

        // Nightly, rc, and feature builds all read gamma; the stable gate is
        // irrelevant to them.
        for gate in [false, true] {
            for ch in [Channel::Nightly, Channel::Rc, feature.clone()] {
                assert_eq!(
                    resolve(false, &ch, gate, None).as_deref(),
                    Some(GAMMA_FEED_URL),
                    "{ch:?} should read gamma"
                );
            }
        }

        // Stable fetches prod only when the rollout gate is on.
        assert_eq!(
            resolve(false, &Channel::Stable, true, None).as_deref(),
            Some(PROD_FEED_URL)
        );
        assert_eq!(resolve(false, &Channel::Stable, false, None), None);

        // Local dev builds never fetch, gate on or off.
        for gate in [false, true] {
            assert_eq!(resolve(false, &Channel::Dev, gate, None), None);
        }

        // Kill switch beats every other input, including an explicit override.
        for ch in [Channel::Nightly, Channel::Stable] {
            assert_eq!(resolve(true, &ch, true, None), None);
            assert_eq!(resolve(true, &ch, true, Some("https://example.com/f.json")), None);
        }

        // The override only redirects a fetch the gates already authorized, and
        // must still be https (loopback excepted).
        assert_eq!(
            resolve(false, &Channel::Nightly, false, Some("https://example.com/f.json")).as_deref(),
            Some("https://example.com/f.json")
        );
        assert_eq!(
            resolve(false, &Channel::Stable, false, Some("https://example.com/f.json")),
            None,
            "override must not bypass the stable rollout gate"
        );
        assert_eq!(
            resolve(false, &Channel::Nightly, true, Some("http://example.com/f.json")),
            None,
            "plaintext override is rejected"
        );
        assert_eq!(
            resolve(false, &Channel::Nightly, true, Some("http://127.0.0.1:1/f.json")).as_deref(),
            Some("http://127.0.0.1:1/f.json"),
            "loopback http is allowed for tests"
        );
    }

    #[test]
    fn test_postprocess_remote_feed() {
        let body = r#"{
            "entries": [
                { "type": "release", "date": "2026-01-03", "version": "2.0.0", "title": "Big", "changes": [] },
                { "type": "release", "date": "2026-01-02", "version": "1.2.0", "changes": [] },
                { "type": "release", "date": "2026-01-01", "version": "1.1.0", "changes": [] },
                { "type": "release", "date": "2026-01-01", "version": "1.X.X", "changes": [] }
            ]
        }"#;

        let versions_for = |current: &str| -> Vec<String> {
            let filtered = Feed::postprocess_remote_feed(body, current).unwrap();
            let feed: Feed = serde_json::from_str(&filtered).unwrap();
            feed.entries.into_iter().map(|e| e.version).collect()
        };

        // Entries newer than the binary are dropped; unparseable (wildcard)
        // versions are kept for consumers to match themselves.
        assert_eq!(versions_for("1.2.0"), ["1.2.0", "1.1.0", "1.X.X"]);

        // A nightly prerelease sorts below the stable release it precedes.
        assert_eq!(versions_for("2.0.0-nightly.1"), ["1.2.0", "1.1.0", "1.X.X"]);

        // Fields not modeled by the Feed struct survive filtering.
        let filtered = Feed::postprocess_remote_feed(body, "2.0.0").unwrap();
        assert!(filtered.contains(r#""title":"Big""#));

        // Unparseable current version keeps all entries.
        assert_eq!(versions_for("not-a-version").len(), 4);

        // Non-feed JSON is rejected.
        assert_eq!(Feed::postprocess_remote_feed(r#"{"foo": 1}"#, "1.0.0"), None);
        assert_eq!(Feed::postprocess_remote_feed("not json", "1.0.0"), None);
    }

    #[test]
    fn test_sanitize_json_strings() {
        let body = r#"{
            "entries": [{
                "type": "release",
                "date": "2026-01-01",
                "version": "1.0.0",
                "changes": [{ "type": "added", "description": "evil \u001b]0;pwned\u0007 \u001b[31mred\nsafe\tok" }]
            }]
        }"#;
        let processed = Feed::postprocess_remote_feed(body, "1.0.0").unwrap();
        let feed: Feed = serde_json::from_str(&processed).unwrap();
        let description = &feed.entries[0].changes[0].description;
        assert_eq!(description, "evil ]0;pwned [31mred\nsafe\tok");
    }

    /// Single test covering all remote-fetch and cache cases sequentially
    /// because they share the KIRO_FEED_URL / KIRO_DATA_DIR /
    /// KIRO_VERSION_OVERRIDE process env vars.
    #[tokio::test]
    async fn test_fetch_remote_json() {
        // Isolate the on-disk cache from the developer's real data dir and
        // pin the channel so assertions don't depend on the build environment.
        // SAFETY: exclusive env access via the shared lock.
        let _guard = crate::util::paths::ENV_MUTATION_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let data_dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("KIRO_DATA_DIR", data_dir.path()) };
        unsafe { std::env::set_var(KIRO_VERSION_OVERRIDE, "9.9.9-nightly.1") };

        // Nightly gets the gamma URL by default; KIRO_FEED_URL only
        // redirects it. A dev build never fetches regardless of rollout state,
        // so it is the order-independent choice here (the stable gate depends
        // on the process-global rollout, covered by resolve_remote_url tests).
        assert_eq!(Feed::remote_url().as_deref(), Some(GAMMA_FEED_URL));
        unsafe { std::env::set_var(KIRO_VERSION_OVERRIDE, "9.9.9-dev") };
        unsafe { std::env::set_var(KIRO_FEED_URL, "https://example.com/feed.json") };
        assert_eq!(Feed::remote_url(), None);
        assert_eq!(Feed::fetch_remote_json().await, None);
        assert_eq!(Feed::load_cached_json(), include_str!("./feed.json"));
        unsafe { std::env::set_var(KIRO_VERSION_OVERRIDE, "9.9.9-nightly.1") };

        let mut server = mockito::Server::new_async().await;
        unsafe { std::env::set_var(KIRO_FEED_URL, format!("{}/feed.json", server.url())) };

        // Valid remote feed with an ETag: returned, parsed, and cached raw.
        let mock = server
            .mock("GET", "/feed.json")
            .with_status(200)
            .with_header("etag", "\"v1\"")
            .with_body(sample_feed_json())
            .create_async()
            .await;
        let body = Feed::fetch_remote_json().await.expect("fetch should succeed");
        let feed: Feed = serde_json::from_str(&body).unwrap();
        assert_eq!(feed.get_version_changelog("0.1.0").unwrap().changes.len(), 1);
        mock.assert_async().await;

        // The raw body and ETag were cached; the cache serves without a fetch.
        assert_eq!(
            std::fs::read_to_string(crate::util::paths::feed_cache_json_path().unwrap()).unwrap(),
            sample_feed_json()
        );
        assert!(Feed::load_cached().get_version_changelog("0.1.0").is_some());

        // Revalidation sends If-None-Match; a 304 serves the cache.
        let mock = server
            .mock("GET", "/feed.json")
            .match_header("if-none-match", "\"v1\"")
            .with_status(304)
            .create_async()
            .await;
        let body = Feed::fetch_remote_json().await.expect("304 should serve cache");
        assert!(body.contains("0.1.0"));
        mock.assert_async().await;

        // Non-success status: fetch fails, load_remote serves the cache.
        let mock = server
            .mock("GET", "/feed.json")
            .with_status(500)
            .expect(2)
            .create_async()
            .await;
        assert_eq!(Feed::fetch_remote_json().await, None);
        assert!(Feed::load_remote().await.get_version_changelog("0.1.0").is_some());
        mock.assert_async().await;

        // Self-heal: a 304 whose cached body is unusable drops the cache and
        // ETag so the next fetch is unconditional.
        std::fs::write(crate::util::paths::feed_cache_json_path().unwrap(), "corrupt").unwrap();
        let mock = server
            .mock("GET", "/feed.json")
            .match_header("if-none-match", "\"v1\"")
            .with_status(304)
            .create_async()
            .await;
        assert_eq!(Feed::fetch_remote_json().await, None);
        assert!(!crate::util::paths::feed_cache_json_path().unwrap().exists());
        assert!(!crate::util::paths::feed_cache_etag_path().unwrap().exists());
        mock.assert_async().await;

        // Oversize body (streamed, no Content-Length) is rejected.
        let big = format!("{{\"entries\": [\"{}\"]}}", "x".repeat(3 * 1024 * 1024));
        let mock = server
            .mock("GET", "/feed.json")
            .with_status(200)
            .with_chunked_body(move |w| std::io::Write::write_all(w, big.as_bytes()))
            .create_async()
            .await;
        assert_eq!(Feed::fetch_remote_json().await, None);
        mock.assert_async().await;

        // Background refresh fetches and writes the cache off the hot path.
        let mock = server
            .mock("GET", "/feed.json")
            .with_status(200)
            .with_body(sample_feed_json())
            .create_async()
            .await;
        Feed::refresh_cache_in_background();
        for _ in 0..100 {
            if crate::util::paths::feed_cache_json_path().unwrap().exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(Feed::load_cached().get_version_changelog("0.1.0").is_some());
        mock.assert_async().await;

        // Kill switch disables the fetch entirely even with the URL override.
        unsafe { std::env::set_var(KIRO_NO_REMOTE_CHANGELOG, "1") };
        assert_eq!(Feed::remote_url(), None);
        assert_eq!(Feed::load_cached_json(), include_str!("./feed.json"));
        unsafe { std::env::remove_var(KIRO_NO_REMOTE_CHANGELOG) };

        // Embedded floor, against an explicit bundled fixture so the result
        // does not depend on the compile-time feed: a cache older than the
        // bundled feed is not served; a newer cache is.
        let bundled_fixture = data_dir.path().join("bundled.json");
        std::fs::write(
            &bundled_fixture,
            r#"{ "entries": [ { "type": "release", "date": "2026-01-02", "version": "2.0.0", "changes": [] } ] }"#,
        )
        .unwrap();
        unsafe { std::env::set_var(KIRO_BUNDLED_FEED_FILE, &bundled_fixture) };

        // The cache is scoped to its source URL, so seed it with whatever
        // remote_url() currently resolves to (the mock server here).
        let active_url = Feed::remote_url().expect("nightly + override resolves a URL");
        let older =
            r#"{ "entries": [ { "type": "release", "date": "2026-01-01", "version": "1.0.0", "changes": [] } ] }"#;
        Feed::write_cache(older, None, &active_url);
        // cache 1.0.0 < bundled 2.0.0 -> floor fires, bundled served.
        assert_eq!(Feed::load_cached().get_version_changelog("2.0.0").is_some(), true);
        assert!(Feed::load_cached().get_version_changelog("1.0.0").is_none());

        let newer =
            r#"{ "entries": [ { "type": "release", "date": "2026-01-03", "version": "3.0.0", "changes": [] } ] }"#;
        Feed::write_cache(newer, None, &active_url);
        // cache 3.0.0 >= bundled 2.0.0 -> cache served.
        assert!(Feed::load_cached().get_version_changelog("3.0.0").is_some());
        unsafe { std::env::remove_var(KIRO_BUNDLED_FEED_FILE) };

        // Invalid feed JSON: fetch fails; with the cache removed, load_remote
        // falls back to the bundled feed, and no If-None-Match is sent.
        let mock = server
            .mock("GET", "/feed.json")
            .match_header("if-none-match", mockito::Matcher::Missing)
            .with_status(200)
            .with_body("not json")
            .expect(2)
            .create_async()
            .await;
        std::fs::remove_file(crate::util::paths::feed_cache_json_path().unwrap()).unwrap();
        assert_eq!(Feed::fetch_remote_json().await, None);
        assert_eq!(Feed::load_remote().await.entries.len(), Feed::load().entries.len());
        mock.assert_async().await;

        // Plaintext transport is rejected unless the host is exactly loopback.
        unsafe { std::env::set_var(KIRO_FEED_URL, "http://example.com/feed.json") };
        assert_eq!(Feed::remote_url(), None);
        unsafe { std::env::set_var(KIRO_FEED_URL, "http://localhost.evil.com/feed.json") };
        assert_eq!(Feed::remote_url(), None);

        // A cache written by another distribution (channel switch) is not
        // served: the recorded source must match the active URL.
        Feed::write_cache(sample_feed_json(), None, "https://elsewhere.example/feed.json");
        assert_eq!(Feed::load_cached_json(), include_str!("./feed.json"));

        unsafe { std::env::remove_var(KIRO_FEED_URL) };
        unsafe { std::env::remove_var(KIRO_VERSION_OVERRIDE) };
        unsafe { std::env::remove_var("KIRO_DATA_DIR") };
    }
}
