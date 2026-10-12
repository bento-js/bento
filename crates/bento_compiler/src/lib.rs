//! TypeScript, JSX, and TSX transformation for direct source execution.
//!
//! The runtime hands each `.ts`, `.mts`, `.cts`, `.tsx`, or `.jsx` module to [`transform`]
//! and evaluates the returned JavaScript. [`CompilerSettings::load`] reads the project's
//! `tsconfig.json` once so the transform honors the execution-relevant options (JSX runtime
//! and import source, import elision, decorators, class fields). Nothing is type checked and
//! no ECMAScript syntax is downleveled.

mod diagnostics;
mod position;
mod settings;
mod syntax_survey;
mod transform;
mod transform_options;
mod tsconfig;
mod tsconfig_diagnostics;

pub use diagnostics::{Diagnostic, Severity};
pub use settings::{CompilerSettings, JsxMode};
pub use transform::{TransformError, Transformed, transform};
pub use tsconfig::ConfigError;
