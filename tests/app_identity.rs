//! F-079: the macOS binary carries an embedded Info.plist so AppKit names the app "Glide".

#![cfg(target_os = "macos")]

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn macos_binary_embeds_info_plist_naming_glide() {
    let bytes = std::fs::read(env!("CARGO_BIN_EXE_glide")).expect("read built glide binary");
    assert!(contains(&bytes, b"<key>CFBundleName</key>"));
    assert!(contains(&bytes, b"<string>Glide</string>"));
    assert!(contains(&bytes, b"<key>CFBundleIconFile</key>"));
}
