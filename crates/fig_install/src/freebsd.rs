use tokio::sync::mpsc::Sender;

use crate::index::UpdatePackage;
use crate::{
    Error,
    UpdateStatus,
};

pub(crate) async fn update(
    _package: UpdatePackage,
    _tx: Sender<UpdateStatus>,
    _interactive: bool,
    _relaunch_dashboard: bool,
) -> Result<(), Error> {
    Err(Error::PackageManaged)
}
