//! The Rust side of the guest book embedding example: storage the host owns, the
//! `host:guestbook` module that exposes it, the runtime configuration that grants a guest
//! book instance exactly what it needs, and the host-owned loop that serves one.

pub mod launch;
pub mod lifecycle;
pub mod module;
pub mod store;
