use std::path::PathBuf;

use objc2_foundation::NSBundle;

pub fn get_bundle_path() -> Option<PathBuf> {
    let main = NSBundle::mainBundle();
    let url = unsafe { main.bundleURL() };
    let path = unsafe { url.path() }?;
    Some(path.to_string().into())
}

pub fn get_bundle_path_for_executable(executable: &str) -> Option<PathBuf> {
    get_bundle_path().and_then(|path| {
        let full_path = path.join("Contents").join("MacOS").join(executable);
        if full_path.exists() { Some(full_path) } else { None }
    })
}

pub fn get_bundle_resource_path() -> Option<PathBuf> {
    get_bundle_path().map(|path| path.join("Contents").join("Resources"))
}
