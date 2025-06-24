use std::fs;

use apple_bundle::prelude::{
    InfoPlist as AppleInfoPlist,
    *,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Serialize, Deserialize)]
struct CargoToml {
    package: CargoTomlPackage,
}

#[derive(Debug, Serialize, Deserialize)]
struct CargoTomlPackage {
    metadata: CargoTomlPackageMetadata,
}

#[derive(Debug, Serialize, Deserialize)]
struct CargoTomlPackageMetadata {
    bundle: BundleConfig,
}

#[derive(Debug, Serialize, Deserialize)]
struct BundleConfig {
    #[serde(rename(serialize = "CFBundleIdentifier"))]
    bundle_identifier: String,
    #[serde(rename(serialize = "CFBundleName"))]
    bundle_name: String,
    // #[serde(rename(serialize = "CFBundleShortVersionString"))]
    // version: String,
    #[serde(flatten)]
    input_method: InputMethod,
}

#[derive(Debug, Serialize, Deserialize)]
struct InfoPlist {
    #[serde(flatten)]
    apple: AppleInfoPlist,
    #[serde(flatten)]
    input_method: InputMethod,
}

#[derive(Debug, Serialize, Deserialize)]
struct InputMethod {
    #[serde(
        rename(serialize = "InputMethodConnectionName"),
        serialize_with = "serialize_option",
        skip_serializing_if = "Option::is_none"
    )]
    input_method_connection_name: Option<String>,
    #[serde(
        rename(serialize = "InputMethodServerControllerClass"),
        default = "input_method_server_controller_class_default"
    )]
    input_method_server_controller_class: String,
    #[serde(rename(serialize = "TISInputSourceID"))]
    input_source_identifier: String,
    #[serde(rename(serialize = "InputMethodType"))]
    input_method_type: String,
    #[serde(rename(serialize = "ComponentInvisibleInSystemUI"))]
    invisible_in_system_ui: bool,
    #[serde(
        rename(serialize = "tsInputMethodIconFileKey"),
        default = "input_method_file_icon_key_default"
    )]
    input_method_file_icon_key: String,
    #[serde(rename(serialize = "TISIntendedLanguage"), default = "intended_language_default")]
    intended_language: String,
}

fn serialize_option<S, T>(value: &Option<T>, ser: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: Serialize,
{
    value
        .as_ref()
        .expect(r#"`serialize_option` must be used with `skip_serializing_if = "Option::is_none"`"#)
        .serialize(ser)
}

fn input_method_server_controller_class_default() -> String {
    "RustInputMethodServerController".to_string()
}

fn input_method_file_icon_key_default() -> String {
    "AppIcon".to_string()
}

fn intended_language_default() -> String {
    "en".to_string()
}

fn main() {
    // println!("cargo:warning=Running build.rs");
    // Tell Cargo that if the given file changes, to rerun this build script.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");

    // Read Cargo.toml and load [bundle]
    let config = fs::read_to_string("Cargo.toml").unwrap();

    let manifest = toml::from_str::<CargoToml>(&config).unwrap();
    let mut bundle = manifest.package.metadata.bundle;

    // Set InputMethodServerControllerClass env var which is used by Input Method at compile time
    println!(
        "cargo:rustc-env=InputMethodServerControllerClass={}",
        bundle.input_method.input_method_server_controller_class
    );

    // Derieve InputMethodConnectionName from bundle identifier
    let connection_name = match bundle.input_method.input_method_connection_name {
        Some(name) => name,
        None => format!("{}_Connection", bundle.bundle_identifier),
    };

    bundle.input_method.input_method_connection_name = Some(connection_name.clone());

    println!("cargo:rustc-env=InputMethodConnectionName={connection_name}");

    let properties = InfoPlist {
        apple: AppleInfoPlist {
            localization: Localization {
                bundle_development_region: Some("en".to_owned()),
                ..Default::default()
            },
            launch: Launch {
                bundle_executable: Some(env!("CARGO_PKG_NAME").to_owned()),
                ..Default::default()
            },
            identification: Identification {
                bundle_identifier: bundle.bundle_identifier,
                ..Default::default()
            },
            bundle_version: BundleVersion {
                bundle_version: Some(env!("CARGO_PKG_VERSION").to_string()),
                bundle_info_dictionary_version: Some("6.0".to_string()),
                bundle_short_version_string: Some(env!("CARGO_PKG_VERSION").to_string()),
                human_readable_copyright: Some(format!(
                    "Copyright © 2022 {} All rights reserved.",
                    env!("CARGO_PKG_AUTHORS")
                )),
            },
            naming: Naming {
                bundle_name: Some(bundle.bundle_name),
                ..Default::default()
            },
            categorization: Categorization {
                bundle_package_type: Some("APPL".to_owned()),
                ..Default::default()
            },
            background_execution: BackgroundExecution {
                background_only: Some(true),
                ..Default::default()
            },
            termination: Termination {
                supports_sudden_termination: Some(true),
                ..Default::default()
            },
            operating_system_version: OperatingSystemVersion {
                minimum_system_version: Some("10.13".to_string()),
                ..Default::default()
            },
            ..Default::default()
        },
        input_method: bundle.input_method,
    };

    // println!("cargo:warning=Regenerating Info.plist");

    // Create Info.plist file
    let file = std::fs::File::create("Info.plist").unwrap();
    plist::to_writer_xml(file, &properties).unwrap();
}
