//! ```cargo
//! [dependencies]
//! wish-tree = { git = "https://github.com/helgoboss/wish-tree", rev = "e2ad6df8e313e46b87"}
//! chrono = "0.4"
//! ```
extern crate chrono;
extern crate wish_tree;

use std::path::*;
use wish_tree::*;

fn main() {
    let dest_dir = PathBuf::from("target");
    #[cfg(target_os = "macos")]
    package_for_macos(&dest_dir);
    #[cfg(target_os = "linux")]
    package_for_linux(&dest_dir);
    #[cfg(target_os = "windows")]
    package_for_windows(&dest_dir);
}

#[cfg(target_os = "macos")]
fn package_for_macos(dest_dir: &Path) {
    let dist = reaper_dir_tree(dir! {
        "ReaLearn.vst" => dir! {
            "Contents" => dir! {
                "MacOS" => dir! {
                    "ReaLearn" => "target/release/librealearn.dylib"
                },
                "Info.plist" => text(generate_plist_info()),
            }
        }
    });
    dist.render_to_zip(dest_dir.join(final_name("zip")));
}

#[cfg(target_os = "linux")]
fn package_for_linux(dest_dir: &Path) {
    let dist = reaper_dir_tree(dir! {
        "ReaLearn.so" => "target/release/librealearn.so"
    });
    dist.render_to_tar_gz(dest_dir.join(final_name("tar.gz")));
}

#[cfg(target_os = "windows")]
fn package_for_windows(dest_dir: &Path) {
    let dist = reaper_dir_tree(dir! {
        "ReaLearn.dll" => "target/release/realearn.dll",
    });
    dist.render_to_zip(dest_dir.join(final_name("zip")));
}

fn reaper_dir_tree(realearn_dir_tree: MountSource) -> MountSource {
    dir! {
        "REAPER" => dir! {
            "UserPlugins" => dir! {
                "FX" => dir! {
                    "ReaLearn" => realearn_dir_tree
                }
            },
        },
    }
}

fn final_name(ext: &str) -> String {
    format!(
        "realearn-v{version}-portable-{os}-{arch}.{ext}",
        version = env!("CARGO_MAKE_CRATE_VERSION"),
        os = env!("CARGO_MAKE_RUST_TARGET_OS"),
        arch = env!("CARGO_MAKE_RUST_TARGET_ARCH"),
        ext = ext,
    )
}

fn generate_plist_info() -> String {
    use chrono::prelude::*;
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple Computer//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>English</string>
	<key>CFBundleExecutable</key>
	<string>Realearn</string>
	<key>CFBundleGetInfoString</key>
	<string>{version}, Copyright {year} Benjamin Klum, Helgoboss Projects</string>
	<key>CFBundleIdentifier</key>
	<string>com.helgoboss.vst.realearn</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>Realearn</string>
	<key>CFBundlePackageType</key>
	<string>BNDL</string>
	<key>CFBundleShortVersionString</key>
	<string>{version}</string>
	<key>CFBundleSignature</key>
	<string>hbpt</string>
	<key>CFBundleVersion</key>
	<string>{version}</string>
	<key>LSMinimumSystemVersion</key>
	<string>10.7.0</string>
</dict>
</plist>
"#,
        version = env!("CARGO_MAKE_CRATE_VERSION"),
        year = Utc::now().year()
    )
}
