//! Build script: on macOS, embeds an Info.plist (app name "Glide") into the `glide` binary only
//! (not the `compat` tool).

use std::env;
use std::fs;
use std::path::PathBuf;

const TEMPLATE: &str = "assets/macos/Info.plist.in";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={TEMPLATE}");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let template = fs::read_to_string(TEMPLATE).expect("Info.plist template is part of the repo");
    let version = env::var("CARGO_PKG_VERSION").expect("cargo sets CARGO_PKG_VERSION");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let plist = out_dir.join("Info.plist");
    fs::write(&plist, template.replace("@VERSION@", &version)).expect("OUT_DIR is writable");
    println!(
        "cargo:rustc-link-arg-bin=glide=-Wl,-sectcreate,__TEXT,__info_plist,{}",
        plist.display()
    );
}
