use std::{env, fs, path::PathBuf};

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set"));
    let expected = fs::read_to_string(root.join("VERSION"))
        .expect("VERSION exists")
        .trim()
        .to_string();
    let actual = env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION is set");

    println!("cargo:rerun-if-changed=VERSION");

    if expected != actual {
        panic!("VERSION mismatch: VERSION has {expected}, Cargo.toml has {actual}");
    }
}
