use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set"));
    let version_path = manifest_dir.join("../..").join("VERSION");
    let expected = fs::read_to_string(&version_path)
        .expect("VERSION exists")
        .trim()
        .to_string();
    let actual = env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION is set");

    println!("cargo:rerun-if-changed={}", version_path.display());

    if expected != actual {
        panic!("VERSION mismatch: VERSION has {expected}, gui/src-tauri/Cargo.toml has {actual}");
    }

    tauri_build::build()
}
