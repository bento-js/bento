//! Build-time checks for the optional native backend.
//!
//! The crate itself builds anywhere: a target without the Apple backend simply
//! reports the model as unavailable. Where the backend is actually compiled,
//! this script refuses an SDK or deployment target below the supported floor
//! and records the SDK that was used.

use std::process::Command;

/// Lowest macOS release and SDK the native backend supports.
const FLOOR: u32 = 27;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=DEVELOPER_DIR");
    println!("cargo::rerun-if-env-changed=SDKROOT");
    println!("cargo::rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");
    // One name for "the Apple backend is compiled in", so the crate does not
    // repeat the feature-and-target condition at every use.
    println!("cargo::rustc-check-cfg=cfg(native_backend)");

    let native = std::env::var_os("CARGO_FEATURE_FOUNDATION_MODELS").is_some()
        && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
        && std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("aarch64");
    if !native {
        return;
    }
    println!("cargo::rustc-cfg=native_backend");

    if let Ok(requested) = std::env::var("MACOSX_DEPLOYMENT_TARGET") {
        let major = major_version(&requested);
        assert!(
            major.is_some_and(|major| major >= FLOOR),
            "MACOSX_DEPLOYMENT_TARGET is `{requested}`, but the Foundation Models backend \
             requires macOS {FLOOR}.0 or newer"
        );
    }

    let sdk = sdk_version().expect(
        "the Foundation Models backend needs the macOS SDK; `xcrun --sdk macosx \
         --show-sdk-version` did not report a version",
    );
    let major = major_version(&sdk);
    assert!(
        major.is_some_and(|major| major >= FLOOR),
        "the macOS SDK is {sdk}, but the Foundation Models backend requires SDK {FLOOR}.0 or \
         newer; select it with DEVELOPER_DIR or SDKROOT"
    );

    println!("cargo::rustc-env=BENTO_PROMPT_NATIVE_SDK_VERSION={sdk}");
}

fn sdk_version() -> Option<String> {
    let output =
        Command::new("xcrun").args(["--sdk", "macosx", "--show-sdk-version"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!version.is_empty()).then_some(version)
}

fn major_version(version: &str) -> Option<u32> {
    version.split('.').next()?.parse().ok()
}
