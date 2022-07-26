use std::borrow::Cow;
use std::sync::Arc;

use fig_util::Terminal;
pub use sentry::integrations::anyhow::capture_anyhow;
pub use sentry::{
    configure_scope,
    release_name,
};

use crate::util::telemetry_is_disabled;

pub fn init_sentry(release: Option<Cow<'static, str>>, project: &str) -> Option<sentry::ClientInitGuard> {
    if std::env::var_os("FIG_DISABLE_SENTRY").is_some() {
        None
    } else {
        let guard = sentry::init((project, sentry::ClientOptions {
            release,
            before_send: Some(Arc::new(
                |event| {
                    if telemetry_is_disabled() { None } else { Some(event) }
                },
            )),
            ..sentry::ClientOptions::default()
        }));

        #[cfg(target_os = "macos")]
        let terminal = Terminal::parent_terminal().map(|s| s.to_string());
        #[cfg(not(target_os = "macos"))]
        let terminal: Option<Terminal> = None;

        sentry::configure_scope(|scope| {
            scope.set_user(Some(sentry::User {
                email: fig_auth::get_email(),
                ..sentry::User::default()
            }));

            if let Some(terminal) = terminal {
                scope.set_tag("terminal", terminal);
            }
        });

        Some(guard)
    }
}
