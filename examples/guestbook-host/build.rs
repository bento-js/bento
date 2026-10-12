use std::path::Path;

fn main() {
    // The guest book's native compiler addons resolve Node-API symbols from the executable that
    // loaded them, so the binary must export them like Deno's own executables do.
    deno_napi::print_linker_flags("bento-guestbook-host");

    // Test executables embed the runtime too. `print_linker_flags` only covers a named binary,
    // so they export the same list from the runtime crate's copy.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let symbols = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/bento_runtime/napi-symbols-macos.def")
            .canonicalize()
            .expect("the runtime crate's Node-API symbol list");
        println!("cargo:rerun-if-changed={}", symbols.display());
        println!("cargo:rustc-link-arg-tests=-Wl,-exported_symbols_list,{}", symbols.display());
    }
}
