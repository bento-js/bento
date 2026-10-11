fn main() {
    // The guest book's native compiler addons resolve Node-API symbols from the executable that
    // loaded them, so the binary must export them like Deno's own executables do.
    deno_napi::print_linker_flags("bento-guestbook-host");
}
