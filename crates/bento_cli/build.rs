fn main() {
    deno_napi::print_linker_flags("bento");
    if std::env::var_os("CARGO_FEATURE_PROMPT_FOUNDATION_MODELS").is_some()
        && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
        && std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("aarch64")
    {
        println!("cargo:rustc-link-arg-bin=bento=-mmacosx-version-min=27.0");
    }
}
