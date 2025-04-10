use objc2_app_kit::NSWorkspace;
use objc2_foundation::{
    NSString,
    NSURL,
};

#[derive(Debug)]
pub struct MacOSApplication {
    pub name: Option<String>,
    pub bundle_identifier: Option<String>,
    pub bundle_path: Option<String>,
    pub process_identifier: libc::pid_t,
}

impl MacOSApplication {
    pub fn terminate(&self) {
        use nix::sys::signal::{
            self,
            Signal,
        };
        use nix::unistd::Pid;

        signal::kill(Pid::from_raw(self.process_identifier), Signal::SIGTERM).ok();
    }
}

pub fn running_applications() -> Vec<MacOSApplication> {
    unsafe {
        let workspace = NSWorkspace::sharedWorkspace();
        let apps = workspace.runningApplications();
        apps.iter()
            .map(|app| {
                let name = app.localizedName().map(|s| s.to_string());
                let bundle_identifier = app.bundleIdentifier().map(|s| s.to_string());
                let bundle_path = app.bundleURL().and_then(|url| url.path()).map(|s| s.to_string());
                let process_identifier = app.processIdentifier();

                MacOSApplication {
                    name,
                    bundle_identifier,
                    bundle_path,
                    process_identifier,
                }
            })
            .collect()
    }
}

pub fn running_applications_matching(bundle_identifier: &str) -> Vec<MacOSApplication> {
    running_applications()
        .into_iter()
        .filter(|app| matches!(&app.bundle_identifier, Some(bundle_id) if bundle_id.as_str() == bundle_identifier))
        .collect()
}

pub fn launch_application(bundle_path: &str) {
    let bundle_nsstring = NSString::from_str(bundle_path);
    let bundle_nsurl = unsafe { NSURL::fileURLWithPath_isDirectory(&bundle_nsstring, true) };

    let workspace = unsafe { NSWorkspace::sharedWorkspace() };
    unsafe { workspace.openURL(&bundle_nsurl) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_running_applications() {
        let applications = running_applications();
        println!("{:#?}", applications);
    }
}

// launch_application("/Applications/Alacritty.app")

// #[test]
// fn test_terminate() {
//     // let out = dbg!(running_applications());
//     running_applications_matching("com.amazon.codewhisperercursor").into_iter().for_each(|app| {
//         println!("Terminating {}", app.process_identifier);
//         app.terminate()
//     })
// }
