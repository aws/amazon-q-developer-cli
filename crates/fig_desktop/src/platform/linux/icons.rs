use std::borrow::Cow;
use std::process::Command;
use std::sync::Mutex;

use anyhow::Result;
use fig_settings::settings;
use tracing::{
    error,
    info,
    warn,
};

use crate::protocol::icons::{
    ProcessedAsset,
    process_asset,
};

static SELECTED_THEME: Mutex<Cow<'_, str>> = Mutex::new(Cow::Borrowed("hicolor"));

pub fn init() -> Result<()> {
    let mut use_local = true;

    if let Some(theme) = settings::get_string("autocomplete.iconTheme")? {
        if theme != "system" {
            use_local = set_theme(theme).is_err();
        }
    }

    if use_local {
        // attempt to get icon theme from gsettings
        match Command::new("gsettings")
            .arg("get")
            .arg("org.gnome.desktop.interface")
            .arg("icon-theme")
            .output()
        {
            Ok(output) => {
                if let Ok(output) = String::from_utf8(output.stdout) {
                    let _ = set_theme(output.split_at(1).1.split_at(output.len() - 3).0.to_string());
                }
            },
            Err(err) => error!(?err, "unable to get icon theme from gsettings"),
        }
    }

    info!("selected theme {}", get_theme());

    Ok(())
}

fn set_theme(theme: String) -> Result<()> {
    if freedesktop_icons::list_themes().contains(&theme.as_str()) || theme == "hicolor" {
        *SELECTED_THEME.lock().unwrap() = Cow::Owned(theme);
        Ok(())
    } else {
        warn!("invalid theme: {theme}");
        Err(anyhow::anyhow!("Invalid theme"))
    }
}

fn get_theme() -> String {
    SELECTED_THEME.lock().unwrap().to_string()
}

pub(super) async fn lookup(name: &str) -> Option<ProcessedAsset> {
    if let Some(path) = freedesktop_icons::lookup(name)
        .with_theme(&get_theme())
        .with_cache()
        .find()
    {
        match process_asset(path.clone()).await {
            Ok(s) => Some(s),
            Err(err) => {
                error!("failed processing asset at {path:?}: {err:?}");
                None
            },
        }
    } else {
        None
    }
}
