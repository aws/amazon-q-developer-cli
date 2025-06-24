use std::io::Result;
use std::path::PathBuf;
use std::process::Command;

#[allow(dead_code)]
enum Version {
    V1([u32; 3]),
    V2([u32; 2]),
}

/// Try to find the version of protoc installed on the system.
fn protoc_version() -> Option<Version> {
    let output = std::process::Command::new("protoc").arg("--version").output().ok()?;
    let version = String::from_utf8(output.stdout).ok()?;
    eprintln!("protoc version: {version:?}");

    let version = version.trim();
    eprintln!("version: {version:?}");
    let version = version.split(' ').last().expect("No version");
    let version = version.split('.').collect::<Vec<_>>();
    let version = version
        .iter()
        .map(|s| s.parse::<u32>().ok())
        .collect::<Option<Vec<_>>>()?;
    match version.len() {
        3 => Some(Version::V1([version[0], version[1], version[2]])),
        2 => Some(Version::V2([version[0], version[1]])),
        _ => None,
    }
}

fn download_protoc() {
    let protoc_version = "26.1";
    let tmp_folder = tempfile::tempdir().unwrap();

    #[cfg(not(windows))]
    download_protoc_unix(protoc_version, &tmp_folder);

    #[cfg(windows)]
    download_protoc_windows(protoc_version, &tmp_folder);
}

#[cfg(not(windows))]
fn download_protoc_unix(protoc_version: &str, tmp_folder: &tempfile::TempDir) {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "osx",
        os => panic!("Unsupported os: {os}"),
    };

    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch_64",
        arch => panic!("Unsupported arch: {arch}"),
    };

    let checksum = match (os, arch) {
        ("linux", "x86_64") => "a7be2928c0454f132c599e25b79b7ad1b57663f2337d7f7e468a1d59b98ec1b0",
        ("linux", "aarch_64") => "64a3b3b5f7dac0c8f9cf1cb85b2b1a237eb628644f6bcb0fb8f23db6e0d66181",
        ("osx", "x86_64") => "febd8821c3a2a23f72f4641471e0ab6486f4fb07b68111490a27a31681465b3c",
        ("osx", "aarch_64") => "26a29befa8891ecc48809958c909d284f2b9539a2eb47f22cadc631fe6abe8fd",
        _ => unreachable!(),
    };

    let mut download_command = Command::new("curl");
    download_command
        .arg("-Lf")
        .arg(format!(
            "https://github.com/protocolbuffers/protobuf/releases/download/v{protoc_version}/protoc-{protoc_version}-{os}-{arch}.zip"
        ))
        .arg("-o")
        .arg(tmp_folder.path().join("protoc.zip"));
    assert!(download_command.spawn().unwrap().wait().unwrap().success());

    let mut checksum_command = Command::new("sha256sum");
    checksum_command.arg(tmp_folder.path().join("protoc.zip"));
    let checksum_output = checksum_command.output().unwrap();
    let checksum_output = String::from_utf8(checksum_output.stdout).unwrap();

    eprintln!("checksum: {checksum_output:?}");
    assert!(checksum_output.starts_with(checksum));

    let mut unzip_command = Command::new("unzip");
    unzip_command
        .arg("-o")
        .arg(tmp_folder.path().join("protoc.zip"))
        .current_dir(tmp_folder.path());
    assert!(unzip_command.spawn().unwrap().wait().unwrap().success());

    let out_bin = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("protoc");

    let mut mv = Command::new("mv");
    mv.arg(tmp_folder.path().join("bin/protoc")).arg(&out_bin);
    assert!(mv.spawn().unwrap().wait().unwrap().success());

    std::env::set_var("PROTOC", out_bin);
}

#[cfg(windows)]
fn download_protoc_windows(protoc_version: &str, tmp_folder: &tempfile::TempDir) {
    // Determine Windows architecture (win32 or win64)
    let win_arch = match std::env::consts::ARCH {
        "x86_64" => "win64",
        "x86" => "win32",
        arch => panic!("Unsupported Windows architecture: {arch}"),
    };

    // Windows-specific checksums
    let checksum = match win_arch {
        "win64" => "9090d135a1159042b13b4e51b210e40cb820d85a5032a6eca5f9b3ca3bdfb539",
        "win32" => "11fc8f280922e86d917e30f7b9960a1e77453f64990d965080697b394a8d9d74",
        _ => unreachable!(),
    };

    // Windows-specific URL format
    let download_url = format!(
        "https://github.com/protocolbuffers/protobuf/releases/download/v{protoc_version}/protoc-{protoc_version}-{win_arch}.zip"
    );

    eprintln!("Downloading protoc from: {download_url}");

    // Download using curl (assuming curl is available on Windows)
    let mut download_command = Command::new("curl");
    download_command
        .arg("-Lf")
        .arg(download_url)
        .arg("-o")
        .arg(tmp_folder.path().join("protoc.zip"));
    assert!(download_command.spawn().unwrap().wait().unwrap().success());

    // Verify checksum using PowerShell
    let mut checksum_command = Command::new("powershell");
    checksum_command.arg("-Command").arg(format!(
        "(Get-FileHash -Path '{}' -Algorithm SHA256).Hash.ToLower()",
        tmp_folder.path().join("protoc.zip").display()
    ));
    let checksum_output = checksum_command.output().unwrap();
    let checksum_output = String::from_utf8(checksum_output.stdout).unwrap().trim().to_lowercase();

    eprintln!("checksum: {checksum_output:?}");
    assert_eq!(
        checksum_output,
        checksum.to_lowercase(),
        "Checksum verification failed. Expected: {}, Got: {}",
        checksum.to_lowercase(),
        checksum_output
    );

    // Extract using PowerShell
    let mut unzip_command = Command::new("powershell");
    unzip_command.arg("-Command").arg(format!(
        "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
        tmp_folder.path().join("protoc.zip").display(),
        tmp_folder.path().display()
    ));
    assert!(unzip_command.spawn().unwrap().wait().unwrap().success());

    // Set output path with .exe extension for Windows
    let out_bin = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("protoc.exe");

    // Copy the protoc binary using PowerShell
    let mut copy_command = Command::new("powershell");
    copy_command.arg("-Command").arg(format!(
        "Copy-Item -Path '{}' -Destination '{}'",
        tmp_folder.path().join("bin").join("protoc.exe").display(),
        out_bin.display()
    ));
    assert!(copy_command.spawn().unwrap().wait().unwrap().success());

    std::env::set_var("PROTOC", out_bin);
}

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");

    let proto_files = std::fs::read_dir("../../proto")?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "proto"))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();

    for file in &proto_files {
        println!("cargo:rerun-if-changed={}", file.display());
    }

    // --experimental_allow_proto3_optional is supported only on version of protoc >= 3.12
    // if the version of the system protoc is too old, we must panic
    match protoc_version() {
        Some(Version::V1([0..=2, _, _] | [3, 0..=11, _])) => download_protoc(),
        Some(Version::V1(_) | Version::V2(_)) => {},
        None => download_protoc(),
    };

    let mut config = prost_build::Config::new();

    config.protoc_arg("--experimental_allow_proto3_optional");

    #[cfg(feature = "arbitrary")]
    config.type_attribute(
        ".",
        "#[cfg_attr(feature = \"arbitrary\", derive(arbitrary::Arbitrary))]",
    );

    config.extern_path(".fig_common.Empty", "()");

    prost_reflect_build::Builder::new()
        .file_descriptor_set_path(PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("file_descriptor_set.bin"))
        .descriptor_pool("crate::DESCRIPTOR_POOL")
        .compile_protos_with_config(config, &proto_files, &["../../proto"])?;

    Ok(())
}
