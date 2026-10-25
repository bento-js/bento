//! Web Prompt API sessions backed by an on-device language model.
//!
//! A session is one conversation with its own history, context budget and
//! lifetime. Availability, compiled backend support and host permission are
//! separate facts: this crate answers the first two, and a host decides the
//! third before calling it.
//!
//! The default build depends on neither a JavaScript engine nor Apple's
//! frameworks, and reports the model as unavailable. The `foundation-models`
//! feature compiles the macOS backend on Apple silicon; the `deno` feature
//! exposes the extension used by Bento's runtime.
//!
//! ```no_run
//! # async fn example() -> Result<(), bento_prompt::Error> {
//! use bento_prompt::{Availability, Cancellation, CreateOptions, PromptOptions, Session};
//!
//! if bento_prompt::availability(&CreateOptions::default()).await? == Availability::Available {
//!     let session = Session::create(CreateOptions::default(), Cancellation::new()).await?;
//!     let answer = session
//!         .prompt(Vec::new(), PromptOptions::default(), Cancellation::new())
//!         .await?;
//!     println!("{answer}");
//!     session.dispose().await;
//! }
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

mod cancel;
mod engine;
mod error;
mod executor;
mod latch;
mod message;
mod schema;
mod session;

#[cfg(test)]
mod tests;

#[cfg(feature = "deno")]
pub mod deno;

use std::sync::Arc;

pub use cancel::Cancellation;
pub use error::{Error, ErrorKind};
pub use message::{Availability, CreateOptions, Expected, Message, PromptOptions, Role};
#[cfg(test)]
pub(crate) use session::pending_create_with;
pub use session::{Disposal, Generation, Session};

/// Ambient TypeScript declarations for the exposed JavaScript API, for
/// embedders that write them into a guest project.
pub const TYPESCRIPT_DECLARATIONS: &str = include_str!("../types/prompt.d.ts");

/// Lowest macOS release the native backend supports.
pub const MACOS_DEPLOYMENT_FLOOR: &str = "27.0";

/// macOS SDK version this build's native backend was compiled against, when
/// the backend is compiled in.
pub const NATIVE_SDK_VERSION: Option<&str> = option_env!("BENTO_PROMPT_NATIVE_SDK_VERSION");

/// Whether the on-device model can serve the described configuration.
///
/// The answer is advisory: [`Session::create`] checks again and may still fail.
/// A build without a compiled backend reports [`Availability::Unavailable`].
///
/// # Errors
///
/// Reports [`ErrorKind::InvalidInput`] for a malformed language tag, and the
/// model's own failure when readiness cannot be determined.
pub async fn availability(options: &CreateOptions) -> Result<Availability, Error> {
    // Readiness depends on the declared expectations only, so the history is
    // not copied for it.
    session::pending_availability(CreateOptions {
        initial_prompts: Vec::new(),
        expected_inputs: options.expected_inputs.clone(),
        expected_outputs: options.expected_outputs.clone(),
    })
    .await
}

pub(crate) async fn availability_with(
    engine: Arc<dyn engine::Engine>,
    options: &CreateOptions,
) -> Result<Availability, Error> {
    message::validate_languages(options)?;
    if !message::expects_text_only(options) {
        return Ok(Availability::Unavailable);
    }
    engine.availability(options).await
}
