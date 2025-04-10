// This is needed for objc
#![allow(unexpected_cfgs)]

use std::borrow::Cow;
use std::io::ErrorKind;
use std::path::{
    Path,
    PathBuf,
};
use std::process::Command;
use std::ptr;

use async_trait::async_trait;
use core_foundation::array::{
    CFArray,
    CFArrayRef,
};
use core_foundation::base::{
    Boolean,
    CFGetTypeID,
    CFType,
    CFTypeID,
    CFTypeRef,
    OSStatus,
    TCFType,
    TCFTypeRef,
};
use core_foundation::boolean::CFBoolean;
use core_foundation::bundle::{
    CFBundle,
    CFBundleRef,
};
use core_foundation::dictionary::{
    CFDictionary,
    CFDictionaryRef,
};
use core_foundation::string::{
    CFString,
    CFStringRef,
};
use core_foundation::url::{
    CFURL,
    CFURLRef,
};
use core_foundation::{
    declare_TCFType,
    impl_TCFType,
};
use fig_settings::state;
use fig_util::Terminal;
use fig_util::consts::CLI_BINARY_NAME;
use fig_util::directories::home_dir;
use fig_util::macos::BUNDLE_CONTENTS_HELPERS_PATH;
use macos_utils::applications;
use objc::runtime::Object;
use objc::{
    class,
    msg_send,
    sel,
    sel_impl,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::fs;
use tracing::{
    debug,
    info,
    trace,
};

use crate::Integration;
use crate::error::{
    ErrorExt,
    Result,
};

pub enum __TISInputSource {}
pub type TISInputSourceRef = *const __TISInputSource;

declare_TCFType! {
    TISInputSource, TISInputSourceRef
}
impl_TCFType!(TISInputSource, TISInputSourceRef, TISInputSourceGetTypeID);

// https://github.com/phracker/MacOSX-SDKs/blob/master/MacOSX10.6.sdk/System/Library/Frameworks/Carbon.framework/Versions/A/Frameworks/HIToolbox.framework/Versions/A/Headers/TextInputSources.h
#[link(name = "Carbon", kind = "framework")]
extern "C" {
    pub static kTISPropertyBundleID: CFStringRef;
    pub static kTISPropertyInputSourceCategory: CFStringRef;
    pub static kTISPropertyInputSourceType: CFStringRef;
    pub static kTISPropertyInputSourceID: CFStringRef;
    pub static kTISPropertyInputSourceIsEnabled: CFStringRef;
    pub static kTISPropertyInputSourceIsSelected: CFStringRef;
    pub static kTISPropertyInputSourceIsEnableCapable: CFStringRef;
    pub static kTISPropertyInputSourceIsSelectCapable: CFStringRef;
    pub static kTISPropertyLocalizedName: CFStringRef;
    pub static kTISPropertyInputModeID: CFStringRef;

    // Can not be used as properties to filter TISCreateInputSourceList
    pub static kTISCategoryKeyboardInputSource: CFStringRef;

    pub static kTISNotifySelectedKeyboardInputSourceChanged: CFStringRef;

    pub static kTISNotifyEnabledKeyboardInputSourcesChanged: CFStringRef;

    pub fn TISInputSourceGetTypeID() -> CFTypeID;

    pub fn TISCreateInputSourceList(properties: CFDictionaryRef, include_all_installed: bool) -> CFArrayRef;

    pub fn TISGetInputSourceProperty(input_source: TISInputSourceRef, property_key: CFStringRef) -> CFTypeRef;

    pub fn TISSelectInputSource(input_source: TISInputSourceRef) -> OSStatus;

    pub fn TISDeselectInputSource(input_source: TISInputSourceRef) -> OSStatus;

    pub fn TISEnableInputSource(input_source: TISInputSourceRef) -> OSStatus;

    pub fn TISDisableInputSource(input_source: TISInputSourceRef) -> OSStatus;

    pub fn TISRegisterInputSource(location: CFURLRef) -> OSStatus;
}

pub struct InputMethod {
    pub bundle_path: PathBuf,
}

use thiserror::Error;

#[derive(Error, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum InputMethodError {
    #[error("Could not list input sources")]
    CouldNotListInputSources,
    #[error("No input sources for bundle identifier {:?}", .identifier)]
    NoInputSourcesForBundleIdentifier { identifier: Cow<'static, str> },
    #[error("Invalid input method bundle destination")]
    InvalidDestination,
    #[error("Invalid path to bundle. Perhaps use an absolute path instead?")]
    InvalidBundlePath,
    #[error("Invalid input method bundle: {}", .inner)]
    InvalidBundle { inner: Cow<'static, str> },
    #[error("OSStatus error code: {0}")]
    OSStatusError(OSStatus),
    #[error("Input source is not enabled")]
    NotEnabled,
    #[error("Input source is not selected")]
    NotSelected,
    #[error("Input method not running")]
    NotRunning,
    #[error("An unknown error occurred")]
    UnknownError,
    #[error("Not installed")]
    NotInstalled,
}

#[macro_export]
macro_rules! tis_action {
    ($action:ident, $function:ident) => {
        pub fn $action(&self) -> Result<(), InputMethodError> {
            debug!("{} input source.", stringify!($action));
            unsafe {
                match $function(self.as_concrete_TypeRef()) {
                    0 => Ok(()),
                    i => Err(InputMethodError::OSStatusError(i).into()),
                }
            }
        }
    };
}

#[macro_export]
macro_rules! tis_property {
    ($name:ident, $tis_property_key:expr, $cf_type:ty, $rust_type:ty, $convert:ident) => {
        #[allow(dead_code)]
        pub fn $name(&self) -> Option<$rust_type> {
            trace!("Get '{}' from input source", stringify!($name));
            self.get_property::<$cf_type>($tis_property_key)
                .map(|s| s.$convert())
        }
    };
}

#[macro_export]
macro_rules! tis_bool_property {
    ($name:ident, $tis_property_key:expr) => {
        tis_property!($name, $tis_property_key, CFBoolean, bool, into);
    };
}

#[macro_export]
macro_rules! tis_string_property {
    ($name:ident, $tis_property_key:expr) => {
        tis_property!($name, $tis_property_key, CFString, String, to_string);
    };
}

impl TISInputSource {
    tis_string_property!(bundle_id, unsafe { kTISPropertyBundleID });

    tis_string_property!(input_source_id, unsafe { kTISPropertyInputSourceID });

    tis_string_property!(category, unsafe { kTISPropertyInputSourceCategory });

    tis_string_property!(localized_name, unsafe { kTISPropertyLocalizedName });

    tis_string_property!(input_mode_id, unsafe { kTISPropertyInputModeID });

    tis_string_property!(category_keyboard, unsafe { kTISCategoryKeyboardInputSource });

    tis_bool_property!(is_enabled, unsafe { kTISPropertyInputSourceIsEnabled });

    tis_bool_property!(is_enable_capable, unsafe { kTISPropertyInputSourceIsEnableCapable });

    tis_bool_property!(is_selected, unsafe { kTISPropertyInputSourceIsSelected });

    tis_bool_property!(is_select_capable, unsafe { kTISPropertyInputSourceIsSelectCapable });

    tis_action!(enable, TISEnableInputSource);

    tis_action!(disable, TISDisableInputSource);

    tis_action!(select, TISSelectInputSource);

    tis_action!(deselect, TISDeselectInputSource);

    // TODO: change to use FromVoid
    fn get_property<T: TCFType>(&self, key: CFStringRef) -> Option<T> {
        unsafe {
            let value: CFTypeRef = TISGetInputSourceProperty(self.as_concrete_TypeRef(), key);

            if value.is_null() {
                None
            } else if T::type_id() == CFGetTypeID(value) {
                // This has to be under get rule
                // https://github.com/phracker/MacOSX-SDKs/blob/master/MacOSX10.6.sdk/System/Library/Frameworks/Carbon.framework/Versions/A/Frameworks/HIToolbox.framework/Versions/A/Headers/TextInputSources.h#L695
                let value = <T::Ref as TCFTypeRef>::from_void_ptr(value);
                Some(T::wrap_under_get_rule(value))
            } else {
                None
            }
        }
    }
}

impl std::fmt::Debug for TISInputSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TISInputSource")
            .field("bundle_id", &self.bundle_id())
            .field("input_source_id", &self.input_source_id())
            .field("input_source_category", &self.category())
            .field("input_source_is_enabled", &self.is_enabled())
            .field("input_source_is_selected", &self.is_selected())
            .field("localized_name", &self.localized_name())
            .field("input_mode_id", &self.input_mode_id())
            .field("category_keyboard", &self.category_keyboard())
            .finish()
    }
}

impl std::default::Default for InputMethod {
    fn default() -> Self {
        let fig_app_path = fig_util::app_bundle_path();
        let bundle_path = fig_app_path
            .join(BUNDLE_CONTENTS_HELPERS_PATH)
            .join("CodeWhispererInputMethod.app");
        Self { bundle_path }
    }
}

impl InputMethod {
    pub fn input_method_directory() -> PathBuf {
        home_dir().unwrap().join("Library").join("Input Methods")
    }

    pub fn list_all_input_sources(
        properties: Option<&CFDictionary<CFType, CFType>>,
        include_all_installed: bool,
    ) -> Option<Vec<TISInputSource>> {
        let properties: CFDictionaryRef = match properties {
            Some(properties) => properties.as_concrete_TypeRef(),
            None => ptr::null(),
        };

        let sources = unsafe { TISCreateInputSourceList(properties, include_all_installed) };
        if sources.is_null() {
            return None;
        }

        let sources = unsafe { CFArray::<TISInputSource>::wrap_under_create_rule(sources) };

        Some(sources.into_iter().map(|value| value.to_owned()).collect())
    }

    pub fn register(location: impl AsRef<Path>) -> Result<(), InputMethodError> {
        debug!("Registering input source...");

        let url = match CFURL::from_path(location, true) {
            Some(url) => url,
            None => return Err(InputMethodError::InvalidDestination),
        };

        unsafe {
            match TISRegisterInputSource(url.as_concrete_TypeRef()) {
                0 => Ok(()),
                i => Err(InputMethodError::OSStatusError(i)),
            }
        }
    }

    pub fn list_input_sources_for_bundle_id(bundle_id: &str) -> Option<Vec<TISInputSource>> {
        let key: CFString = unsafe { CFString::wrap_under_create_rule(kTISPropertyBundleID) };
        let value = CFString::from(bundle_id);
        let properties = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);

        InputMethod::list_all_input_sources(Some(&properties), true)
    }
}

extern "C" {
    pub fn CFBundleGetIdentifier(bundle: CFBundleRef) -> CFStringRef;
    pub fn CFPreferencesSynchronize(
        application_id: CFStringRef,
        username: CFStringRef,
        hostname: CFStringRef,
    ) -> Boolean;
    pub static kCFPreferencesCurrentUser: CFStringRef;
    pub static kCFPreferencesCurrentHost: CFStringRef;
}

#[link(name = "AppKit", kind = "framework")]
extern "C" {}

impl InputMethod {
    pub fn input_source(&self) -> Result<TISInputSource, InputMethodError> {
        let bundle_id_string: String = self.bundle_id()?;
        let bundle_identifier = CFString::from(bundle_id_string.as_str());

        unsafe {
            let bundle_id_key: CFString = CFString::wrap_under_get_rule(kTISPropertyBundleID);
            let category_key: CFString = CFString::wrap_under_get_rule(kTISPropertyInputSourceCategory);
            let input_source_key: CFString = CFString::wrap_under_get_rule(kTISPropertyInputSourceID);

            let properties = CFDictionary::from_CFType_pairs(&[
                (bundle_id_key.as_CFType(), bundle_identifier.as_CFType()),
                (
                    category_key.as_CFType(),
                    CFString::from_static_string("TISCategoryPaletteInputSource").as_CFType(),
                ),
                (input_source_key.as_CFType(), bundle_identifier.as_CFType()),
            ]);

            let sources = InputMethod::list_all_input_sources(Some(&properties), true);

            match sources {
                None => Err(InputMethodError::CouldNotListInputSources),
                Some(sources) => {
                    let len = sources.len();
                    match len {
                        0 => Err(InputMethodError::NoInputSourcesForBundleIdentifier {
                            identifier: bundle_identifier.to_string().into(),
                        }),
                        _ => sources.into_iter().next().ok_or_else(|| {
                            InputMethodError::NoInputSourcesForBundleIdentifier {
                                identifier: bundle_identifier.to_string().into(),
                            }
                        }),
                    }
                },
            }
        }
    }

    pub fn target_bundle_path(&self) -> Result<PathBuf, InputMethodError> {
        let input_method_name = match self.bundle_path.components().last() {
            Some(name) => name.as_os_str(),
            None => {
                return Err(InputMethodError::InvalidBundle {
                    inner: "Input method bundle name cannot be determined".into(),
                });
            },
        };

        Ok(InputMethod::input_method_directory().join(input_method_name))
    }

    pub fn bundle_id(&self) -> Result<String, InputMethodError> {
        let url = match CFURL::from_path(&self.bundle_path, true) {
            Some(url) => url,
            None => {
                return Err(InputMethodError::InvalidBundle {
                    inner: "Could not get URL for input method bundle".into(),
                });
            },
        };

        let bundle = match CFBundle::new(url) {
            Some(bundle) => bundle,
            None => {
                return Err(InputMethodError::InvalidBundle {
                    inner: format!("Could not load bundle for URL {}", self.bundle_path.display()).into(),
                });
            },
        };

        let identifier = unsafe { CFBundleGetIdentifier(bundle.as_concrete_TypeRef()) };

        if identifier.is_null() {
            return Err(InputMethodError::InvalidBundle {
                inner: "Could find bundle identifier".into(),
            });
        }

        let bundle_identifier = unsafe { CFString::wrap_under_get_rule(identifier) };

        Ok(bundle_identifier.to_string())
    }

    pub fn launch(&self) {
        debug!("Launching input method...");

        if let Some(bundle_path) = self.bundle_path.to_str() {
            applications::launch_application(bundle_path);
        }
    }

    pub fn terminate(&self) -> Result<(), InputMethodError> {
        debug!("Terminating input method...");

        let bundle_id = self.bundle_id()?;
        applications::running_applications_matching(&bundle_id)
            .iter()
            .for_each(|app| app.terminate());

        Ok(())
    }
}

fn str_to_nsstring(str: &str) -> &Object {
    const UTF8_ENCODING: usize = 4;
    unsafe {
        let ns_string: &mut Object = msg_send![class!(NSString), alloc];
        let ns_string: &mut Object = msg_send![
            ns_string,
            initWithBytes: str.as_ptr()
            length: str.len()
            encoding: UTF8_ENCODING
        ];
        let _: () = msg_send![ns_string, autorelease];
        ns_string
    }
}

#[async_trait]
impl Integration for InputMethod {
    async fn is_installed(&self) -> Result<()> {
        // let attr = fs::metadata(&self.bundle_path)?;
        let destination = self.target_bundle_path()?;

        // check that symlink to input method exists in input_methods_directory
        let symlink = fs::read_link(destination).await;

        match symlink {
            Ok(symlink) => {
                // does it point to the correct location
                if symlink != self.bundle_path {
                    return Err(InputMethodError::InvalidBundle {
                        inner: "Symbolic link is incorrect".into(),
                    }
                    .into());
                }
            },
            Err(err) if err.kind() == ErrorKind::NotFound => return Err(InputMethodError::NotInstalled.into()),
            Err(err) => return Err(err.into()),
        }

        // check that the input method is running (NSRunning application)
        let bundle_id = self.bundle_id()?;
        if applications::running_applications_matching(bundle_id.as_str()).is_empty() {
            return Err(InputMethodError::NotRunning.into());
        }

        // Can we load input source?

        // todo: pull this into a function in fig_directories
        let cli_path = fig_util::app_bundle_path()
            .join("Contents")
            .join("MacOS")
            .join(CLI_BINARY_NAME);

        let out = tokio::process::Command::new(cli_path)
            .args(["_", "attempt-to-finish-input-method-installation"])
            .arg(&self.bundle_path)
            .output()
            .await
            .with_context(|err| format!("Could not run {CLI_BINARY_NAME} cli: {err}"))?;

        if out.status.code() == Some(0) {
            self.set_is_enabled(true);
            Ok(())
        } else {
            self.set_is_enabled(false);
            let err = String::from_utf8_lossy(&out.stdout);
            match serde_json::from_str::<InputMethodError>(&err).ok() {
                Some(error) => Err(error.into()),
                None => Err(InputMethodError::UnknownError.into()),
            }
        }
    }

    async fn install(&self) -> Result<()> {
        {
            let destination = self.target_bundle_path()?;

            // Attempt to emove existing symlink
            fs::remove_file(&destination).await.ok();

            // Create the parent directory if it doesn't exist
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .await
                    .with_context(|_| format!("Could not create directory {}", parent.display()))?;
            }

            // Create new symlink
            fs::symlink(&self.bundle_path, &destination)
                .await
                .with_context(|_| format!("Could not create symlink {}", destination.display()))?;

            // Register input source
            InputMethod::register(&destination)?;

            debug!("Launch Input Method...");
            if let Some(dest) = destination.to_str() {
                Command::new("open").arg(dest);
            }

            run_on_main(|| {
                let source = self.input_source()?;
                source.enable()?;
                Ok::<(), InputMethodError>(())
            })?;

            // The 'enabled' property of an input source is never updated for the process that
            // invokes `TISEnableInputSource` Unclear why this is, but we handle it by
            // calling out to the q_cli to finish the second half of the installation.
        }

        // todo: pull this into a function in fig_directories
        let q_cli_path = fig_util::app_bundle_path()
            .join("Contents")
            .join("MacOS")
            .join(CLI_BINARY_NAME);

        loop {
            let out = tokio::process::Command::new(&q_cli_path)
                .args(["_", "attempt-to-finish-input-method-installation"])
                .arg(&self.bundle_path)
                .output()
                .await
                .with_context(|err| format!("Could not run {CLI_BINARY_NAME} cli: {err}"))?;

            if out.status.code() == Some(0) {
                info!("Input method installed successfully!");
                break;
            } else {
                let err = String::from_utf8_lossy(&out.stdout);
                match serde_json::from_str::<InputMethodError>(&err).ok() {
                    Some(error) => debug!("{error}"),
                    None => debug!("Could not parse output as known error: {err}"),
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        // Store PIDs of all relevant terminal emulators (input method will not work until these
        // processes are restarted)
        for app in applications::running_applications().iter() {
            if let Some(bundle_id) = &app.bundle_identifier {
                match Terminal::from_bundle_id(bundle_id) {
                    Some(terminal) if terminal.supports_macos_input_method() => {
                        state::set_value(
                            self.terminal_instance_requires_restart_key(&terminal, app.process_identifier),
                            true,
                        )
                        .ok();
                    },
                    _ => (),
                }
            }
        }

        self.set_is_enabled(true);

        Ok(())
    }

    async fn uninstall(&self) -> Result<()> {
        self.set_is_enabled(false);

        let destination = self.target_bundle_path()?;

        let binding = run_on_main(|| {
            let input_source = self.input_source()?;
            input_source.deselect()?;
            input_source.disable()?;

            Ok::<_, InputMethodError>(input_source.bundle_id())
        })?;

        let binding = binding.ok_or_else(|| InputMethodError::InvalidBundle {
            inner: "Could not get bundle id".into(),
        })?;

        // todo(mschrage): Terminate input method binary using Cocoa APIs
        unsafe {
            let bundle_id: &Object = str_to_nsstring(binding.as_str());
            let running_input_method_array: &mut Object = msg_send![
                class!(NSRunningApplication),
                runningApplicationsWithBundleIdentifier: bundle_id
            ];
            let running_input_method_array_len: u64 = msg_send![running_input_method_array, count];

            if running_input_method_array_len > 0 {
                let running_input_method: &mut Object = msg_send![running_input_method_array, objectAtIndex: 0];

                let _: () = msg_send![running_input_method, terminate];
            }
        }

        // Remove symbolic link
        fs::remove_file(destination).await?;

        Ok(())
    }

    fn describe(&self) -> String {
        "Input Method".into()
    }

    async fn migrate(&self) -> Result<()> {
        // Check the symlink, if it points at the wrong location update it
        let destination = self.target_bundle_path()?;
        let symlink = fs::read_link(&destination).await;

        match symlink {
            Ok(symlink) => {
                // does it point to the correct location
                if symlink != self.bundle_path {
                    fs::remove_file(&destination).await?;
                    fs::symlink(&self.bundle_path, destination).await?;
                }
            },
            Err(err) if err.kind() == ErrorKind::NotFound => {},
            Err(err) => return Err(err.into()),
        }

        Ok(())
    }
}

impl InputMethod {
    // Called from separate process in order to check status of Input Method
    pub fn finish_input_method_installation(bundle_path: Option<PathBuf>) -> Result<(), InputMethodError> {
        let input_method = match bundle_path {
            Some(bundle_path) if bundle_path.is_absolute() => InputMethod { bundle_path },
            Some(_) => return Err(InputMethodError::InvalidBundlePath),
            None => InputMethod::default(),
        };

        let source = input_method.input_source()?;

        if !source.is_enabled().unwrap_or_default() {
            return Err(InputMethodError::NotEnabled);
        }

        source.select()?;

        if !source.is_selected().unwrap_or_default() {
            return Err(InputMethodError::NotSelected);
        }

        Ok(())
    }

    fn terminal_instance_requires_restart_key(&self, terminal: &Terminal, process_identifier: i32) -> String {
        let input_method_bundle_id = self.bundle_id().ok().unwrap_or_else(|| "unknown-bundle-id".into());
        format!(
            "input-method={}.{}.process[{}]-requires-restart",
            input_method_bundle_id,
            terminal.internal_id(),
            process_identifier
        )
    }

    pub fn enabled_for_terminal_instance(&self, terminal: &Terminal, process_identifier: i32) -> bool {
        let key = self.terminal_instance_requires_restart_key(terminal, process_identifier);
        let requires_restart = state::get_bool_or(&key, false);

        let enabled = !requires_restart;

        if enabled {
            state::remove_value(key).ok();
        }

        enabled
    }

    fn input_method_is_enabled_key(&self) -> String {
        let input_method_bundle_id = self.bundle_id().ok().unwrap_or_else(|| "unknown-bundle-id".into());
        format!("input-method={input_method_bundle_id}.enabled")
    }

    pub fn is_enabled(&self) -> Option<bool> {
        let key = self.input_method_is_enabled_key();
        state::get_bool(key).unwrap_or_default()
    }

    fn set_is_enabled(&self, enabled: bool) {
        let key = self.input_method_is_enabled_key();
        state::set_value(key, enabled).ok();
    }
}

fn run_on_main<T, F>(work: F) -> T
where
    F: Send + FnOnce() -> T,
    T: Send,
{
    cfg_if::cfg_if! {
        if #[cfg(feature = "dispatch")] {
            dispatch::Queue::main().exec_sync(work)
        } else {
            work()
        }
    }
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    const TEST_INPUT_METHOD_BUNDLE_ID: &str = "com.amazon.inputmethod.codewhisperer";
    const TEST_INPUT_METHOD_BUNDLE_URL: &str =
        "/Applications/Amazon Q.app/Contents/Helpers/CodeWhispererInputMethod.app";

    fn input_method() -> TISInputSource {
        let key: CFString = unsafe { CFString::wrap_under_create_rule(kTISPropertyBundleID) };
        let value = CFString::from_static_string(TEST_INPUT_METHOD_BUNDLE_ID);
        let properties = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
        let sources = InputMethod::list_all_input_sources(Some(&properties), true).unwrap_or_default();
        sources.into_iter().next().unwrap()
    }

    #[ignore]
    #[test]
    fn check_enabled() {
        let method = InputMethod {
            bundle_path: TEST_INPUT_METHOD_BUNDLE_URL.into(),
        };

        println!(
            "{} enabled: {}",
            method.input_source().unwrap().bundle_id().unwrap(),
            method.input_source().unwrap().is_enabled().unwrap()
        );
    }

    #[ignore]
    #[tokio::test]
    async fn install() {
        let method = InputMethod {
            bundle_path: TEST_INPUT_METHOD_BUNDLE_URL.into(),
        };

        let bundle_id = TEST_INPUT_METHOD_BUNDLE_ID;
        match InputMethod::list_input_sources_for_bundle_id(bundle_id) {
            Some(inputs) => {
                println!("Uninstalling...");
                for s in inputs.iter() {
                    println!("{}", s.is_enabled().unwrap_or_default());
                }

                match method.uninstall().await {
                    Ok(_) => println!("Uninstalled!"),
                    Err(e) => println!("{e}"),
                }
            },
            None => {
                println!("No input sources found for {bundle_id}");
                println!("Installing...");
                match method.install().await {
                    Ok(_) => println!("Installed!"),
                    Err(e) => println!("{e}"),
                };
            },
        }
    }

    #[ignore]
    #[test]
    fn toggle_selection() {
        let source = input_method();
        let selected = source.is_selected();
        match selected {
            Some(true) => {
                source.select().ok();
                assert!(source.is_selected().unwrap_or_default());
                source.deselect().ok();
                assert!(!source.is_selected().unwrap_or(true));
                source.select().ok();
                assert!(selected == source.is_selected());
            },
            Some(false) => {
                source.deselect().ok();
                assert!(!source.is_selected().unwrap_or_default());
                source.select().ok();
                assert!(source.is_selected().unwrap_or(false));
                source.deselect().ok();
                assert!(selected == source.is_selected());
            },

            None => unreachable!("Is selected should be defined"),
        }
    }

    #[ignore]
    #[test]
    fn get_input_source_by_bundle_id() {
        let bundle_identifier = TEST_INPUT_METHOD_BUNDLE_ID; //"com.apple.CharacterPaletteIM";
        let sources = InputMethod::list_input_sources_for_bundle_id(bundle_identifier);
        match sources {
            Some(sources) => {
                println!("Found {} matching source", sources.len());
                assert!(sources.len() == 1);
                assert!(sources[0].bundle_id().unwrap() == bundle_identifier);
                assert!(sources[0].category().unwrap() == "TISCategoryPaletteInputSource");

                println!("{:?}", sources[0]);
            },
            None => unreachable!("{} should always exist.", bundle_identifier),
        }
    }

    #[ignore]
    #[test]
    fn uninstall_all() {
        let sources = InputMethod::list_input_sources_for_bundle_id(TEST_INPUT_METHOD_BUNDLE_ID).unwrap_or_default();
        for s in sources.iter() {
            s.deselect().ok();
            s.disable().ok();
        }
    }

    #[ignore]
    #[test]
    fn test_list_all_input_methods() {
        let sources = InputMethod::list_all_input_sources(None, true).unwrap_or_default();

        assert!(!sources.is_empty());
        for source in sources.iter() {
            println!("{source:?}");
        }
    }

    #[test]
    fn serialize_deserialize_error() {
        let error = InputMethodError::InvalidBundle {
            inner: "Invalid bundle".into(),
        };
        let serialized = serde_json::to_string(&error).unwrap();
        println!("invalid_bundle: {serialized}");
        let deserialized: InputMethodError = serde_json::from_str(&serialized).unwrap();
        assert_eq!(error, deserialized);

        let error = InputMethodError::UnknownError;
        let serialized = serde_json::to_string(&error).unwrap();
        println!("unknown_error: {serialized}");
        let deserialized: InputMethodError = serde_json::from_str(&serialized).unwrap();
        assert_eq!(error, deserialized);
    }
}
