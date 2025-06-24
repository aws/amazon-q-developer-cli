extern crate bindgen;

use std::env;
use std::io::Error;
use std::path::Path;
use std::process::Command;

fn get_sdk_path() -> Result<String, Error> {
    let output = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()?
        .stdout;

    let output_str = String::from_utf8(output).expect("Failed to convert xcrun output to string");

    Ok(output_str.trim().to_string())
}

// Using bindgen gets NSWorkspace Bindings
// some of the naming of nsworkspace is a bit different in bindgen
// ex: NSWorkspace.shared.whatever (Swift) --> NSWorkspace::SharedWorkspace::Whatever
pub fn build() {
    let target = std::env::var("TARGET").unwrap();

    let default_sdk_path =
        "/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX12.1.sdk";

    let sdk_path: String = match get_sdk_path() {
        Ok(path) => path,
        Err(e) => {
            println!("cargo:warning=Failed to get MacOSX SDK Path. Trying to using the default one {e:?}",);
            String::from(default_sdk_path)
        },
    };

    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=Foundation");

    let builder = bindgen::Builder::default()
        .formatter(bindgen::Formatter::Rustfmt)
        .header_contents(
            "NSWorkspace.h",
            "
            #include<AppKit/NSWorkspace.h>
            #include<AppKit/NSRunningApplication.h>
        ",
        )
        .clang_arg(format!("--target={target}"))
        .clang_args(&["-isysroot", sdk_path.as_ref()])
        .block_extern_crate(true)
        .objc_extern_crate(true)
        .clang_arg("-ObjC")
        .blocklist_item("objc_object");

    let bindings = builder.generate().expect("Failed to generate bindings");

    let out_dir = env::var_os("OUT_DIR").unwrap();

    bindings
        .write_to_file(Path::new(&out_dir).join("nsworkspace.rs"))
        .expect("Failed to write bindings to file");
}
