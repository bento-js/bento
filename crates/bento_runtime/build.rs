fn main() {
    // Deno v2.9.7's MIT-licensed Node-API export list keeps addon ABI symbols alive in tests.
    // Embedding binaries use deno_napi::print_linker_flags in their own build scripts.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let symbols = std::path::Path::new("napi-symbols-macos.def").canonicalize().unwrap();
        println!("cargo:rerun-if-changed={}", symbols.display());
        println!("cargo:rustc-link-arg-tests=-Wl,-exported_symbols_list,{}", symbols.display());
    }
}
