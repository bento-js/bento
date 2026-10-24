//! Host authority granted to Bento runtime instances.
//!
//! A host decides, before any guest code runs, which files, endpoints, environment
//! variables, and native addons one runtime instance may use. These types carry that
//! decision; the runtime enforces it at its native operation boundaries. Nested Bento
//! launches receive the invoking launch's policy as a ceiling they may only narrow.

mod grants;
mod net;
mod path;
mod scope;

pub use grants::{GrantKind, GrantRequest, Grants, InvalidCeiling, UnencodableCeiling, WiderGrant};
pub use net::{InvalidNetTarget, NetTarget};
pub use path::canonical_path;
pub use scope::{Covers, Scope};

/// Environment variable through which a launch hands its policy to nested Bento
/// launches as their ceiling.
pub const CEILING_ENV: &str = "BENTO_GRANT_CEILING";
