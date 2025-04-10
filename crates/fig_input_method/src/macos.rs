use fig_log::{
    LogArgs,
    initialize_logging,
};
use fig_util::directories;
use objc2::rc::autoreleasepool;
use objc2::runtime::Bool;
use objc2::{
    ClassType,
    msg_send,
};
use objc2_app_kit::NSApp;
use objc2_foundation::{
    MainThreadMarker,
    NSBundle,
    NSObject,
    ns_string,
};
use tracing::info;

use crate::imk;

const CONNECTION_NAME: &str = env!("InputMethodConnectionName");

#[tokio::main]
pub async fn main() {
    let _log_guard = initialize_logging(LogArgs {
        log_level: Some("trace".to_owned()),
        log_to_stdout: true,
        log_file_path: Some(directories::logs_dir().expect("home dir must be set").join("imk.log")),
        delete_old_log_file: false,
    });

    info!("Registering imk controller");
    imk::register_controller();
    info!("Registered imk controller");

    let mtm = MainThreadMarker::new().expect("must be on the main thread");

    autoreleasepool(|_pool| {
        let app = NSApp(mtm);

        let k_connection_name = ns_string!(CONNECTION_NAME);
        let nib_name = ns_string!("MainMenu");

        let bundle = NSBundle::mainBundle();
        let identifier = unsafe { bundle.bundleIdentifier() };

        info!("Attempting connection...");
        imk::connect_imkserver(k_connection_name, identifier.as_deref());
        info!("Connected!");

        let app_id: &NSObject = app.as_ref();
        let loaded_nib: Bool = unsafe { msg_send![NSBundle::class(), loadNibNamed:nib_name owner:app_id] };
        info!("RUNNING {loaded_nib:?}!");

        unsafe { app.run() };
    });
}
