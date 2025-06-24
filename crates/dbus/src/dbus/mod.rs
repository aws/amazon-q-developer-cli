use std::sync::OnceLock;

use fig_os_shim::Context;
use thiserror::Error;
use tokio::sync::Mutex;
use zbus::Connection;

pub use self::ibus::{
    AddressError,
    connect_to_ibus_daemon,
};

pub mod gnome_shell;
pub mod ibus;

#[derive(Debug, Error)]
pub enum CrateError {
    #[error(transparent)]
    Address(#[from] AddressError),
    #[error(transparent)]
    ZBus(#[from] zbus::Error),
    #[error(transparent)]
    ZVariant(#[from] zbus::zvariant::Error),
    #[error("Invalid GNOME shell version {0}")]
    InvalidVersion(String),
    #[error(transparent)]
    Fdo(#[from] zbus::fdo::Error),
}

static SESSION_BUS: OnceLock<Connection> = OnceLock::new();
static SESSION_BUS_INIT: Mutex<()> = Mutex::const_new(());

async fn session_bus() -> Result<&'static Connection, CrateError> {
    if let Some(connection) = SESSION_BUS.get() {
        return Ok(connection);
    }

    let _guard = SESSION_BUS_INIT.lock().await;

    if let Some(connection) = SESSION_BUS.get() {
        return Ok(connection);
    }

    let connection = Connection::session().await?;

    let _ = SESSION_BUS.set(connection);

    Ok(SESSION_BUS.get().unwrap())
}

static IBUS_BUS: OnceLock<Connection> = OnceLock::new();
static IBUS_BUS_INIT: Mutex<()> = Mutex::const_new(());

pub async fn ibus_bus(ctx: &Context) -> Result<&'static Connection, CrateError> {
    if let Some(connection) = IBUS_BUS.get() {
        return Ok(connection);
    }

    let _guard = IBUS_BUS_INIT.lock().await;

    if let Some(connection) = IBUS_BUS.get() {
        return Ok(connection);
    }

    let connection = connect_to_ibus_daemon(ctx).await?;

    let _ = IBUS_BUS.set(connection);

    Ok(IBUS_BUS.get().unwrap())
}
