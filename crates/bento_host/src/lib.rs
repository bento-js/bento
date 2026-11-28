//! The permissions a host gives Bento runtime instances.
//!
//! A host decides, before any guest code runs, what one runtime instance may do: everything
//! except what it denies ([`Permissions::unrestricted`]), or only what it allows
//! ([`Permissions::restricted`]). These types carry that decision, with Node's vocabulary and
//! matching rules; the runtime enforces it at its native operation boundaries. Nested Bento
//! launches receive the invoking launch's permissions as a ceiling they can never widen.

#![warn(missing_docs)]

mod ceiling;
mod env_pattern;
mod path;
mod path_pattern;
mod permission;
mod permission_set;
mod permissions;
mod scope;

pub use ceiling::{Excess, InvalidCeiling, UnencodableCeiling};
pub use env_pattern::{EnvPattern, InvalidEnvPattern, is_always_kept};
pub use path::canonical_path;
pub use path_pattern::PathPattern;
pub use permission::Permission;
pub use permission_set::PermissionSet;
pub use permissions::Permissions;
pub use scope::Scope;

/// Environment variable through which a launch hands its permissions to nested Bento
/// launches as their ceiling.
pub const CEILING_ENV: &str = "BENTO_PERMISSION_CEILING";
