use std::path::PathBuf;
use std::{
    env,
    fs,
};

use image::imageops::FilterType;

fn main() {
    build_assets();
}

fn build_assets() {
    resize_directory(
        "icons/autocomplete",
        "AUTOCOMPLETE_ICONS_PROCESSED",
        32,
        32,
        FilterType::Lanczos3,
    );
}

fn resize_directory(name: &str, var: &str, width: u32, height: u32, filter: FilterType) {
    println!("cargo:rerun-if-changed={name}");
    let source = env::current_dir().unwrap().join(name);
    let target = PathBuf::from(env::var("OUT_DIR").unwrap()).join(name);
    fs::create_dir_all(&target).expect("Failed creating assets folder");

    for (name, path) in source
        .read_dir()
        .expect("Failed reading nested assets directory")
        .map(Result::unwrap)
        .map(|x| (x.file_name(), x.path()))
    {
        let asset = image::open(path.clone())
            .unwrap_or_else(|err| panic!("Failed reading image at path {}: {:?}", &path.to_string_lossy(), err));
        let asset = asset.resize_exact(width, height, filter);
        asset.save(target.join(name)).expect("Failed writing image");
    }

    println!("cargo:rustc-env={}={}", var, target.to_string_lossy());
}
