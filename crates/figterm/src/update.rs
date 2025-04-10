use crossterm::style::Stylize;
use fig_os_shim::{
    Context,
    Os,
};
use fig_settings::settings::get_bool_or;
use fig_telemetry::{
    InstallMethod,
    get_install_method,
};
use fig_util::CLI_BINARY_NAME;
use fig_util::manifest::{
    Variant,
    manifest,
};
use semver::Version;
use tracing::warn;

const UPDATE_AVAILABLE_KEY: &str = "update.new-version-available";

fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap()
}

fn print_update_message(context: &Context, version: &Version) {
    let os = context.platform().os();
    let variant = &manifest().variant;
    match (os, variant) {
        (Os::Linux, Variant::Full) => {
            println!(
                "\nA new version of {} is available: {}\n",
                CLI_BINARY_NAME.bold(),
                version.to_string().bold(),
            );
        },
        _ => {
            println!(
                "\nA new version of {} is available: {}\nRun {} to update to the new version\n",
                CLI_BINARY_NAME.bold(),
                version.to_string().bold(),
                format!("{CLI_BINARY_NAME} update").magenta().bold()
            );
        },
    };
}

pub fn check_for_update(context: &Context) {
    let not_linux = context.platform().os() != Os::Linux;
    let in_cloudshell = context.env().in_cloudshell();
    let autoupdate_disabled = !get_bool_or("app.disableAutoupdates", true);
    let installed_via_toolbox = get_install_method() == InstallMethod::Toolbox;

    // If any of the previous conditions, do not show the update notification
    if not_linux | in_cloudshell | autoupdate_disabled | installed_via_toolbox {
        return;
    }

    tokio::spawn(async {
        match fig_install::check_for_updates(false).await {
            Ok(Some(pkg)) => {
                if let Err(err) = fig_settings::state::set_value(UPDATE_AVAILABLE_KEY, pkg.version.to_string()) {
                    warn!(?err, "Error setting {UPDATE_AVAILABLE_KEY}: {err}");
                }
            },
            Ok(None) => {},
            Err(err) => {
                warn!(?err, "Error checking for updates: {err}");
            },
        };
    });

    match fig_settings::state::get_string(UPDATE_AVAILABLE_KEY) {
        Ok(Some(version)) => match Version::parse(&version) {
            Ok(version) => {
                let current_version = current_version();
                if version > current_version {
                    print_update_message(context, &version);
                }
            },
            Err(err) => {
                warn!(?err, "Error parsing {UPDATE_AVAILABLE_KEY}: {err}");
                let _ = fig_settings::state::remove_value(UPDATE_AVAILABLE_KEY);
            },
        },
        Ok(None) => {},
        Err(err) => {
            warn!(?err, "Error getting {UPDATE_AVAILABLE_KEY}: {err}");
            let _ = fig_settings::state::remove_value(UPDATE_AVAILABLE_KEY);
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_version() {
        let version = current_version();
        println!("Crate version: {version}");
    }

    #[test]
    fn test_print_update_message() {
        let version = Version::parse("1.2.3").unwrap();
        println!("===");
        print_update_message(&Context::new(), &version);
        println!("===");

        println!("===");
        print_update_message(&Context::builder().with_os(Os::Linux).build_fake(), &version);
        println!("===");
    }
}
