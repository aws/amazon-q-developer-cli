//! # DBus interface proxy for: `org.freedesktop.IBus`

use std::path::Path;
use std::process::Output;

use fig_os_shim::{
    Context,
    Fs,
};
use thiserror::Error;
use tracing::{
    debug,
    trace,
};
use zbus::zvariant::{
    OwnedObjectPath,
    OwnedValue,
};
use zbus::{
    Connection,
    ConnectionBuilder,
    proxy,
};

use super::{
    CrateError,
    ibus_bus,
};

#[derive(Debug, Error)]
pub enum AddressError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("Null address")]
    Null,
    #[error("Command failed: {0:?}")]
    FailedOutput(Output),
    #[error("No home directory found")]
    NoHomeDirectory,
    #[error(transparent)]
    ZBusError(#[from] zbus::Error),
    #[error("No address found")]
    NoAddressFound,
}

/// Gets the D-Bus [zbus::Connection] to ibus-daemon.
///
/// Internally, [connect_to_ibus_daemon] checks the env var `IBUS_ADDRESS` if set, and falls back to
/// checking the addresses written under `~/.config/ibus/bus/`.
///
/// Reference: <https://ibus.github.io/docs/ibus-1.5/ibus-ibusshare.html#ibus-get-address>
pub async fn connect_to_ibus_daemon(ctx: &Context) -> Result<Connection, AddressError> {
    let (fs, env) = (ctx.fs(), ctx.env());

    match env.get("IBUS_ADDRESS") {
        Ok(address) => {
            debug!(address, "using ibus address from IBUS_ADDRESS");
            return connect_to_bus_address(&address).await;
        },
        Err(err) => {
            debug!(?err, "IBUS_ADDRESS not set, falling back to ~/.config/ibus/bus");
        },
    }

    let ibus_addresses_dir = env
        .home()
        .ok_or(AddressError::NoHomeDirectory)?
        .join(".config/ibus/bus");
    // There's multiple files on my system but only one is the correct one. Therefore, just try
    // connecting to each one until we find one that succeeds.
    let mut files = fs.read_dir(ibus_addresses_dir).await?;
    while let Some(file) = files.next_entry().await? {
        let path = file.path();

        // Skip processing if not a file.
        match path.metadata() {
            Ok(metadata) => {
                if !metadata.is_file() {
                    trace!("path at {} is not a file, ignoring", path.display());
                    continue;
                }
            },
            Err(err) => {
                trace!(?err, "unable to get metadata for {}, ignoring", path.display());
                continue;
            },
        }

        // Skip if no address was parsed.
        let maybe_address = match parse_address_from_file(fs, &path).await {
            Ok(address) => address,
            Err(err) => {
                trace!(?err, "unable to parse ibus address for {}, ignoring", path.display());
                continue;
            },
        };

        // Skip if couldn't connect to the parsed address.
        match connect_to_bus_address(&maybe_address).await {
            Ok(conn) => return Ok(conn),
            Err(err) => {
                trace!(
                    ?err,
                    maybe_address,
                    "unable to connect to address for {}, ignoring",
                    path.display()
                );
                continue;
            },
        }
    }

    Err(AddressError::NoAddressFound)
}

async fn parse_address_from_file(fs: &Fs, path: impl AsRef<Path>) -> Result<String, AddressError> {
    fs.read_to_string(path)
        .await?
        .lines()
        .skip_while(|s| !s.starts_with(char::is_uppercase))
        .find_map(|s| {
            s.split_once("=").and_then(|(key, value)| {
                if key.trim() == "IBUS_ADDRESS" {
                    Some(value.trim().to_string())
                } else {
                    None
                }
            })
        })
        .ok_or(AddressError::NoAddressFound)
}

async fn connect_to_bus_address(address: &str) -> Result<Connection, AddressError> {
    Ok(ConnectionBuilder::address(address)?.build().await?)
}

pub async fn ibus_proxy(ctx: &Context) -> Result<IBusProxy<'static>, CrateError> {
    Ok(IBusProxy::new(ibus_bus(ctx).await?).await?)
}

#[proxy(interface = "org.freedesktop.IBus", assume_defaults = true)]
pub trait IBus {
    /// CreateInputContext method
    fn create_input_context(&self, client_name: &str) -> zbus::Result<OwnedObjectPath>;

    /// Exit method
    fn exit(&self, restart: bool) -> zbus::Result<()>;

    /// GetEnginesByNames method
    fn get_engines_by_names(&self, names: &[&str]) -> zbus::Result<Vec<OwnedValue>>;

    /// GetUseGlobalEngine method
    fn get_use_global_engine(&self) -> zbus::Result<bool>;

    /// Ping method
    fn ping(&self, data: &zbus::zvariant::Value<'_>) -> zbus::Result<OwnedValue>;

    /// RegisterComponent method
    fn register_component(&self, component: &zbus::zvariant::Value<'_>) -> zbus::Result<()>;

    /// SetGlobalEngine method
    fn set_global_engine(&self, engine_name: &str) -> zbus::Result<()>;

    /// RegistryChanged signal
    #[dbus_proxy(signal)]
    fn registry_changed(&self) -> zbus::Result<()>;

    /// ActiveEngines property
    #[dbus_proxy(property)]
    fn active_engines(&self) -> zbus::Result<Vec<OwnedValue>>;

    /// Address property
    #[dbus_proxy(property)]
    fn address(&self) -> zbus::Result<String>;

    /// CurrentInputContext property
    #[dbus_proxy(property)]
    fn current_input_context(&self) -> zbus::Result<OwnedObjectPath>;

    /// EmbedPreeditText property
    #[dbus_proxy(property)]
    fn embed_preedit_text(&self) -> zbus::Result<bool>;

    #[dbus_proxy(property)]
    fn set_embed_preedit_text(&self, value: bool) -> zbus::Result<()>;

    /// Engines property
    #[dbus_proxy(property)]
    fn engines(&self) -> zbus::Result<Vec<OwnedValue>>;

    /// GlobalEngine property
    #[dbus_proxy(property)]
    fn global_engine(&self) -> zbus::Result<OwnedValue>;

    /// PreloadEngines property
    #[dbus_proxy(property)]
    fn set_preload_engines(&self, value: &[&str]) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[tokio::test]
    async fn test_parse_address_from_file() {
        let fs = Fs::new_chroot();
        let path = PathBuf::from("1a3abbde88dc49cfba0fdf8c86463f9e-unix-0");
        fs.write(
            &path,
            r#"\
# This file is created by ibus-daemon, please do not modify it.
# This file allows processes on the machine to find the
# ibus session bus with the below address.
# If the IBUS_ADDRESS environment variable is set, it will
# be used rather than this file.
IBUS_ADDRESS=unix:abstract=/home/testuser/.cache/ibus/dbus-lO89nVNb,guid=bc4af0ed046ca788302207ed67083bdd
IBUS_DAEMON_PID=1424421"#,
        )
        .await
        .unwrap();

        let actual_address = parse_address_from_file(&fs, &path).await.unwrap();
        assert_eq!(
            actual_address,
            "unix:abstract=/home/testuser/.cache/ibus/dbus-lO89nVNb,guid=bc4af0ed046ca788302207ed67083bdd"
        );
    }

    #[tokio::test]
    #[ignore = "not in ci"]
    async fn test_e2e_connect_to_ibus() {
        let _ = connect_to_ibus_daemon(&Context::new()).await.unwrap();
    }
}
