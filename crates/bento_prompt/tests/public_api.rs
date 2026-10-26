//! Public surface a Rust consumer can rely on without the runtime or V8.

use bento_prompt::Cancellation;

#[cfg(not(feature = "foundation-models"))]
use bento_prompt::CreateOptions;

#[cfg(not(feature = "foundation-models"))]
#[tokio::test]
async fn availability_reports_a_backendless_build_as_unavailable() {
    let reported =
        bento_prompt::availability(&CreateOptions::default()).await.expect("availability resolves");

    assert_eq!(reported, bento_prompt::Availability::Unavailable);
}

#[cfg(not(feature = "foundation-models"))]
#[tokio::test]
async fn creation_without_a_compiled_backend_is_unsupported() {
    let error = bento_prompt::Session::create(CreateOptions::default(), Cancellation::new())
        .await
        .expect_err("a backend-free build cannot create a session");

    assert_eq!(error.kind(), bento_prompt::ErrorKind::NotSupported);
}

#[tokio::test]
async fn a_cancellation_is_shared_by_its_clones() {
    let cancellation = Cancellation::new();
    let clone = cancellation.clone();
    assert!(!cancellation.is_cancelled());

    clone.cancel();

    assert!(cancellation.is_cancelled());
    cancellation.cancelled().await;
    clone.cancel();
    assert!(clone.is_cancelled(), "cancellation is idempotent");
}
