//! Availability, creation, committed history, ordering, forks and disposal.

use std::time::Duration;

use crate::tests::scripted::{EMPTY_OVERHEAD, Event, Script, ScriptedEngine, transcript_tokens};
use crate::tests::{assistant, position, scripted_session, system, user};
use crate::{
    Availability, Cancellation, CreateOptions, ErrorKind, Expected, Message, PromptOptions,
    Session, availability_with,
};

fn committed_of(engine: &ScriptedEngine, index: usize) -> Vec<Message> {
    engine.requests().get(index).expect("recorded request").committed.clone()
}

fn opened(engine: &ScriptedEngine) -> Vec<Vec<Message>> {
    engine
        .log()
        .into_iter()
        .filter_map(|event| match event {
            Event::Opened(history) => Some(history),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn create_commits_initial_prompts_without_generating() {
    let engine = ScriptedEngine::new();
    let initial = vec![system("Answer concisely."), user("Hello"), assistant("Hi there")];
    let options = CreateOptions { initial_prompts: initial.clone(), ..CreateOptions::default() };

    let session = Session::create_with(engine.engine(), options, Cancellation::new())
        .await
        .expect("creation succeeds");

    assert!(engine.requests().is_empty(), "initial history must not generate a reply");
    assert_eq!(opened(&engine), vec![initial.clone()], "roles and order reach the engine intact");
    assert_eq!(session.context_usage(), transcript_tokens(&initial) - 1);
}

#[tokio::test]
async fn unavailable_model_fails_creation_as_unsupported() {
    let engine = ScriptedEngine::new();
    engine.set_available(false);

    let availability = availability_with(engine.engine(), &CreateOptions::default())
        .await
        .expect("availability resolves");
    assert_eq!(availability, Availability::Unavailable);

    let error =
        Session::create_with(engine.engine(), CreateOptions::default(), Cancellation::new())
            .await
            .expect_err("creation fails when the model is unavailable");
    assert_eq!(error.kind(), ErrorKind::NotSupported);
    assert!(opened(&engine).is_empty(), "no native session is opened");
}

#[tokio::test]
async fn unsupported_modality_is_unavailable_without_consulting_the_model() {
    let engine = ScriptedEngine::new();
    let options = CreateOptions {
        expected_inputs: vec![Expected { kind: "image".to_owned(), languages: Vec::new() }],
        ..CreateOptions::default()
    };

    let availability =
        availability_with(engine.engine(), &options).await.expect("availability resolves");

    assert_eq!(availability, Availability::Unavailable);
    assert!(engine.log().is_empty(), "an unsupported modality is decided before the model");

    let error = Session::create_with(engine.engine(), options, Cancellation::new())
        .await
        .expect_err("creation rejects an unsupported modality");
    assert_eq!(error.kind(), ErrorKind::NotSupported);
}

#[tokio::test]
async fn an_expectation_naming_no_content_type_is_unavailable_and_rejects_creation() {
    let engine = ScriptedEngine::new();
    let options =
        CreateOptions { expected_outputs: vec![Expected::default()], ..CreateOptions::default() };

    let availability =
        availability_with(engine.engine(), &options).await.expect("availability resolves");

    assert_eq!(availability, Availability::Unavailable);
    assert!(
        engine.log().is_empty(),
        "an expectation this backend cannot read never reaches the model"
    );

    let error = Session::create_with(engine.engine(), options, Cancellation::new())
        .await
        .expect_err("creation rejects an expectation that names no content type");
    assert_eq!(error.kind(), ErrorKind::NotSupported);
}

#[tokio::test]
async fn unsupported_language_is_unavailable_and_rejects_creation() {
    let engine = ScriptedEngine::new();
    engine.set_languages(["en"]);
    let options = CreateOptions {
        expected_outputs: vec![Expected {
            kind: "text".to_owned(),
            languages: vec!["ja".to_owned()],
        }],
        ..CreateOptions::default()
    };

    let availability =
        availability_with(engine.engine(), &options).await.expect("availability resolves");
    assert_eq!(availability, Availability::Unavailable);

    let error = Session::create_with(engine.engine(), options, Cancellation::new())
        .await
        .expect_err("creation rejects an unsupported language");
    assert_eq!(error.kind(), ErrorKind::NotSupported);
}

#[tokio::test]
async fn append_commits_history_without_inference() {
    let (engine, session) = scripted_session().await;

    session
        .append(
            vec![user("Remember the code word: kelp."), assistant("The code word is kelp.")],
            Cancellation::new(),
        )
        .await
        .expect("append commits");

    assert!(engine.requests().is_empty(), "append must not generate");
    assert_eq!(
        session.context_usage(),
        transcript_tokens(&[
            user("Remember the code word: kelp."),
            assistant("The code word is kelp.")
        ]) - 1
    );

    engine.script(Script::text(["kelp"]));
    session
        .prompt(vec![user("What is the code word?")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("prompt succeeds");

    assert_eq!(
        committed_of(&engine, 0),
        vec![user("Remember the code word: kelp."), assistant("The code word is kelp.")],
        "appended turns keep their roles in the generated request"
    );
}

#[tokio::test]
async fn system_message_after_history_is_rejected_without_changing_state() {
    let (engine, session) = scripted_session().await;
    session.append(vec![user("Hello")], Cancellation::new()).await.expect("append");
    let usage = session.context_usage();
    let events = engine.log().len();

    let error = session
        .append(vec![system("Too late")], Cancellation::new())
        .await
        .expect_err("a later system message is invalid");

    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert_eq!(session.context_usage(), usage, "rejected input changes nothing");
    assert_eq!(engine.log().len(), events, "validation happens before the engine is used");
}

#[tokio::test]
async fn operations_apply_in_invocation_order_not_poll_order() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::text(["ordered"]));

    let append = session.append(vec![user("first")], Cancellation::new());
    let prompt =
        session.prompt(vec![user("second")], PromptOptions::default(), Cancellation::new());

    let answer = prompt.await.expect("prompt resolves");
    append.await.expect("append resolves");

    assert_eq!(answer, "ordered");
    assert_eq!(committed_of(&engine, 0), vec![user("first")], "the earlier call is applied first");
    assert_eq!(engine.requests()[0].prompt, user("second"));
}

#[tokio::test]
async fn fork_snapshots_history_at_its_queue_position() {
    let (engine, session) = scripted_session().await;
    session.append(vec![user("one")], Cancellation::new()).await.expect("append");

    let fork = session.fork(Cancellation::new());
    let later = session.append(vec![user("two")], Cancellation::new());
    let clone = fork.await.expect("fork resolves");
    later.await.expect("later append resolves");

    assert_eq!(clone.context_usage(), transcript_tokens(&[user("one")]) - 1);
    assert_eq!(clone.context_window(), session.context_window());

    engine.script(Script::text(["forked"]));
    clone
        .prompt(vec![user("ask")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("clone generates");

    assert_eq!(
        committed_of(&engine, 0),
        vec![user("one")],
        "the clone carries the snapshot, not later source history"
    );
}

#[tokio::test]
async fn clone_outlives_destruction_of_its_source() {
    let (engine, session) = scripted_session().await;
    session.append(vec![user("shared")], Cancellation::new()).await.expect("append");
    let clone = session.fork(Cancellation::new()).await.expect("fork resolves");

    session.destroy();

    engine.script(Script::text(["independent"]));
    let answer = clone
        .prompt(vec![user("still there?")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("the clone still works");
    assert_eq!(answer, "independent");
}

#[tokio::test]
async fn fork_cancellation_does_not_destroy_the_clone() {
    let (engine, session) = scripted_session().await;
    let cancel = Cancellation::new();
    let clone = session.fork(cancel.clone()).await.expect("fork resolves");

    cancel.cancel();

    engine.script(Script::text(["alive"]));
    let answer = clone
        .prompt(vec![user("alive?")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("the completed clone has its own lifetime");
    assert_eq!(answer, "alive");
}

#[tokio::test]
async fn destroy_is_idempotent_and_fails_later_operations() {
    let (_engine, session) = scripted_session().await;

    session.destroy();
    session.destroy();

    let error = session
        .prompt(vec![user("after destruction")], PromptOptions::default(), Cancellation::new())
        .await
        .expect_err("operations fail after destruction");
    assert_eq!(error.kind(), ErrorKind::Aborted);
    assert_eq!(
        session.append(vec![user("x")], Cancellation::new()).await.unwrap_err().kind(),
        ErrorKind::Aborted
    );
}

#[tokio::test]
async fn creation_cancellation_destroys_the_session() {
    let engine = ScriptedEngine::new();
    let cancel = Cancellation::new();
    let session = Session::create_with(engine.engine(), CreateOptions::default(), cancel.clone())
        .await
        .expect("creation succeeds");

    cancel.cancel();

    let error = session
        .prompt(vec![user("after abort")], PromptOptions::default(), Cancellation::new())
        .await
        .expect_err("the retained creation cancellation destroys the session");
    assert_eq!(error.kind(), ErrorKind::Aborted);
}

#[tokio::test]
async fn dispose_resolves_only_after_native_settlement() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::hang().settle_after_cancel_ms(20));

    let generation = session
        .stream(vec![user("long")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("stream starts");
    engine.wait_for(|events| events.iter().any(|event| matches!(event, Event::Generated(_)))).await;

    session.destroy();
    session.dispose().await;
    drop(generation);

    let events = engine.log();
    let cancelled = position(&events, |event| matches!(event, Event::Cancelled(0)));
    let settled = position(&events, |event| matches!(event, Event::Settled(0)));
    let released = position(&events, |event| matches!(event, Event::Released));
    assert!(cancelled < settled, "cancellation precedes settlement");
    assert!(settled < released, "the native session is released only after settlement");
}

#[tokio::test]
async fn dropping_a_session_cancels_and_releases_native_work() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::hang());

    let generation = session
        .stream(vec![user("long")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("stream starts");
    engine.wait_for(|events| events.iter().any(|event| matches!(event, Event::Generated(_)))).await;

    drop(generation);
    drop(session);

    engine.wait_for(|events| events.contains(&Event::Released)).await;
    let events = engine.log();
    let cancelled = position(&events, |event| matches!(event, Event::Cancelled(0)));
    let settled = position(&events, |event| matches!(event, Event::Settled(0)));
    let released = position(&events, |event| matches!(event, Event::Released));
    assert!(cancelled < settled && settled < released);
}

#[tokio::test]
async fn a_failed_readiness_probe_is_reported_without_retrying() {
    let engine = ScriptedEngine::new();
    engine.fail_next_availability(ErrorKind::Unknown);

    let error =
        Session::create_with(engine.engine(), CreateOptions::default(), Cancellation::new())
            .await
            .expect_err("creation reports the failed readiness probe");
    assert_eq!(error.kind(), ErrorKind::Unknown);

    let session =
        Session::create_with(engine.engine(), CreateOptions::default(), Cancellation::new())
            .await
            .expect("a separate creation can probe readiness again");
    session.dispose().await;
}

#[tokio::test]
async fn creation_infrastructure_failure_is_an_operation_error() {
    let engine = ScriptedEngine::new();
    engine.fail_open(ErrorKind::Unknown);

    let error =
        Session::create_with(engine.engine(), CreateOptions::default(), Cancellation::new())
            .await
            .expect_err("creation reports the infrastructure failure");

    assert_eq!(error.kind(), ErrorKind::Operation);
}

#[tokio::test]
async fn clone_infrastructure_failure_is_an_operation_error() {
    let (engine, session) = scripted_session().await;
    engine.fail_open(ErrorKind::Unknown);

    let error = session.fork(Cancellation::new()).await.expect_err("cloning failed");

    assert_eq!(error.kind(), ErrorKind::Operation);
    session.dispose().await;
}

#[test]
fn disposal_resolves_after_the_runtime_that_started_the_work_is_gone() {
    let engine = ScriptedEngine::new();
    engine.script(Script::hang());
    let worker =
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("worker runtime");

    let disposal = worker.block_on(async {
        let session =
            Session::create_with(engine.engine(), CreateOptions::default(), Cancellation::new())
                .await
                .expect("session");
        let generation = session
            .stream(vec![user("long")], PromptOptions::default(), Cancellation::new())
            .await
            .expect("stream starts");
        engine
            .wait_for(|events| events.iter().any(|event| matches!(event, Event::Generated(0))))
            .await;
        let disposal = session.dispose();
        drop(generation);
        drop(session);
        disposal
    });

    // A worker isolate's runtime disappears while its model work is in flight.
    worker.shutdown_background();

    let host =
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("host runtime");
    host.block_on(async move {
        tokio::time::timeout(Duration::from_secs(5), disposal)
            .await
            .expect("disposal resolves without the runtime that started the work");
    });
}

#[test]
fn disposal_covers_native_work_that_started_before_the_session_existed() {
    let engine = ScriptedEngine::new();
    let hold = engine.hold_open();
    let worker =
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("worker runtime");

    let disposal = worker.block_on(async {
        let pending = crate::pending_create_with(
            engine.engine(),
            CreateOptions::default(),
            Cancellation::new(),
        );
        let disposal = pending.disposal();
        drop(tokio::spawn(pending));
        engine.wait_for(|events| events.contains(&Event::Opening)).await;
        disposal
    });

    // The isolate's runtime dies while the model is still opening a session.
    worker.shutdown_background();
    assert!(!disposal.is_settled(), "the native open is still in flight");

    hold.release();
    let host =
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("host runtime");
    host.block_on(async move {
        tokio::time::timeout(Duration::from_secs(5), disposal)
            .await
            .expect("disposal waits for the abandoned creation and then completes");
    });
    assert!(
        engine.log().contains(&Event::Released),
        "the session opened for an abandoned creation is released"
    );
}

#[tokio::test]
async fn cancelling_during_preflight_commits_nothing_and_never_generates() {
    let (engine, session) = scripted_session().await;
    let hold = engine.hold_counts();
    let cancellation = Cancellation::new();
    let counts_before =
        engine.log().iter().filter(|event| matches!(event, Event::Counting)).count();

    let prompt = session.prompt(vec![user("talk")], PromptOptions::default(), cancellation.clone());
    engine
        .wait_for(|events| {
            events.iter().filter(|event| matches!(event, Event::Counting)).count() > counts_before
        })
        .await;
    cancellation.cancel();
    hold.release();

    let error = prompt.await.expect_err("an operation cancelled during preflight fails");

    assert_eq!(error.kind(), ErrorKind::Aborted);
    assert!(engine.requests().is_empty(), "preflight cancellation never reaches generation");
    assert_eq!(session.context_usage(), 0, "preflight cancellation commits nothing");
}

#[tokio::test]
async fn destroying_a_session_during_the_final_count_commits_no_response() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::text(["Hello there"]));
    let counts_before =
        engine.log().iter().filter(|event| matches!(event, Event::Counting)).count();
    // The preflight count runs; the session is destroyed while the count that
    // decides whether the response is committed is still in flight.
    let hold = engine.hold_counts_after(1);

    let prompt = session.prompt(vec![user("talk")], PromptOptions::default(), Cancellation::new());
    engine
        .wait_for(|events| {
            events.iter().filter(|event| matches!(event, Event::Counting)).count()
                > counts_before + 1
        })
        .await;
    session.destroy();
    hold.release();

    let error = prompt.await.expect_err("destruction during the final count fails the turn");

    assert_eq!(error.kind(), ErrorKind::Aborted);
    assert_eq!(
        session.context_usage(),
        transcript_tokens(&[user("talk")]) - EMPTY_OVERHEAD,
        "the accepted input stays committed and the undelivered response does not"
    );
    session.dispose().await;
}

#[cfg(not(native_backend))]
#[tokio::test]
async fn an_operation_without_a_backend_owns_no_native_work() {
    let pending = crate::session::pending_create(CreateOptions::default(), Cancellation::new());

    assert!(
        pending.disposal().is_settled(),
        "an operation decided without touching the model has nothing to settle"
    );
    let error = pending.await.expect_err("a build without a backend cannot create a session");
    assert_eq!(error.kind(), ErrorKind::NotSupported);
}

/// Without a backend the answer is still "unsupported", but options this crate can refuse on
/// its own are refused first, as they are on the path that reaches a model.
#[cfg(not(native_backend))]
#[tokio::test]
async fn a_build_without_a_backend_refuses_invalid_options_before_reporting_no_backend() {
    let malformed_tag = crate::session::pending_create(expecting("en--US"), Cancellation::new())
        .await
        .expect_err("a malformed language tag cannot create a session");
    assert_eq!(malformed_tag.kind(), ErrorKind::InvalidInput);

    let misplaced_system = CreateOptions {
        initial_prompts: vec![user("Hello"), system("Answer concisely.")],
        ..CreateOptions::default()
    };
    let error = crate::session::pending_create(misplaced_system, Cancellation::new())
        .await
        .expect_err("a misplaced system message cannot create a session");
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
}

fn expecting(tag: &str) -> CreateOptions {
    CreateOptions {
        expected_inputs: vec![Expected {
            kind: "text".to_owned(),
            languages: vec![tag.to_owned()],
        }],
        ..CreateOptions::default()
    }
}

#[tokio::test]
async fn a_malformed_language_tag_is_rejected_without_probing_the_model() {
    for tag in ["123", "e", "", "en-", "-en", "en--US", "abcdefghi"] {
        let engine = ScriptedEngine::new();
        let options = expecting(tag);

        let error = availability_with(engine.engine(), &options)
            .await
            .expect_err("a malformed language tag is invalid input");
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "`{tag}` is not a language tag");
        assert!(engine.log().is_empty(), "`{tag}` is rejected before the model is probed");

        let error = Session::create_with(engine.engine(), options, Cancellation::new())
            .await
            .expect_err("a malformed language tag cannot create a session");
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "`{tag}` is not a language tag");
        assert!(engine.log().is_empty(), "creation does not probe the model for `{tag}`");
    }
}

#[tokio::test]
async fn a_well_formed_language_tag_reaches_the_model() {
    // Support is the model's answer, so validation must not narrow the tags it
    // is allowed to decide on.
    for tag in ["en", "en-US", "zh-Hant-TW", "abcde", "de-CH-1901", "en-US-u-ca-buddhist"] {
        let engine = ScriptedEngine::new();
        engine.set_languages([tag]);

        let availability = availability_with(engine.engine(), &expecting(tag))
            .await
            .unwrap_or_else(|error| panic!("`{tag}` is a well-formed language tag: {error}"));

        assert_eq!(availability, Availability::Available, "`{tag}` is supported by this model");
        assert!(
            engine.log().contains(&Event::AvailabilityChecked),
            "`{tag}` is decided by the model, not by validation"
        );
    }
}
