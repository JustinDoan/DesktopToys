use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let box3d_dir = manifest_dir.join("../extern/box3d");
    let src_dir = box3d_dir.join("src");
    let include_dir = box3d_dir.join("include");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/box3d_backend.c");
    println!("cargo:rerun-if-changed=src/box3d_backend.h");
    println!("cargo:rerun-if-changed={}", box3d_dir.display());

    let mut build = cc::Build::new();
    build
        .std("c17")
        .include(&include_dir)
        .include(&src_dir)
        .define("BOX3D_DISABLE_SIMD", None)
        .define("_CRT_SECURE_NO_WARNINGS", None)
        .file("src/box3d_backend.c");

    for source_file in c_sources(&src_dir) {
        build.file(source_file);
    }

    build.compile("physics_core_box3d");

    if env::var("CARGO_CFG_TARGET_FAMILY").as_deref() == Ok("unix") {
        println!("cargo:rustc-link-lib=m");
    }
}

fn c_sources(src_dir: &Path) -> Vec<PathBuf> {
    let mut files = fs::read_dir(src_dir)
        .unwrap_or_else(|error| panic!("failed to read Box3D source directory {}: {error}", src_dir.display()))
        .map(|entry| entry.expect("failed to read Box3D source entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "c"))
        .collect::<Vec<_>>();
    files.sort();
    files
}
