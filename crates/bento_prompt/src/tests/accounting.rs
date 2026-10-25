//! Context accounting: reserved overhead, measurement, quota and clones.

use serde_json::json;

use crate::tests::scripted::{Event, Script, ScriptedEngine, schema_tokens, transcript_tokens};
use crate::tests::{assistant, scripted_session, user};
use crate::{Cancellation, CreateOptions, ErrorKind, Message, PromptOptions, Session};

const NATIVE_WINDOW: usize = 256;
const EMPTY_OVERHEAD: usize = 1;

#[tokio::test]
async fn an_empty_session_starts_at_zero_usage_within_the_reserved_window() {
    let (_engine, session) = scripted_session().await;

    assert_eq!(session.context_usage(), 0, "the empty transcript overhead is reserved");
    assert_eq!(session.context_window(), NATIVE_WINDOW - EMPTY_OVERHEAD);
}

#[tokio::test]
async fn usage_tracks_the_committed_transcript_across_append_sequences() {
    let (_engine, session) = scripted_session().await;
    let mut committed: Vec<Message> = Vec::new();
    let mut previous = 0;

    for seed in 0..40_u64 {
        let words = usize::try_from(seed % 7).expect("small") + 1;
        let content = vec!["token"; words].join(" ");
        let message = if seed % 2 == 0 { user(&content) } else { assistant(&content) };
        committed.push(message.clone());

        session.append(vec![message], Cancellation::new()).await.expect("append commits");

        let usage = session.context_usage();
        assert_eq!(
            usage,
            transcript_tokens(&committed) - EMPTY_OVERHEAD,
            "usage always equals the measured transcript minus the reserved overhead"
        );
        assert!(usage >= previous, "usage never decreases while history grows");
        previous = usage;
    }
}

#[tokio::test]
async fn measurement_reports_the_addition_without_mutating_the_session() {
    let (engine, session) = scripted_session().await;
    session.append(vec![user("one two")], Cancellation::new()).await.expect("append");
    let usage = session.context_usage();

    let measured = session
        .measure_context_usage(
            vec![user("three four five")],
            PromptOptions::default(),
            Cancellation::new(),
        )
        .await
        .expect("measurement resolves");

    assert_eq!(
        measured,
        transcript_tokens(&[user("one two"), user("three four five")])
            - transcript_tokens(&[user("one two")])
    );
    assert!(measured > 0, "measurement is finite and nonnegative");
    assert_eq!(session.context_usage(), usage, "measurement does not mutate state");
    assert!(engine.requests().is_empty(), "measurement does not generate");
}

/// The draft's measurement validates its input as a conversation of its own, with no
/// appended input, so a system message is measurable on a session that already has history.
#[tokio::test]
async fn measurement_accepts_a_system_message_on_a_session_with_history() {
    let (_engine, session) = scripted_session().await;
    session.append(vec![user("one two")], Cancellation::new()).await.expect("append");
    let usage = session.context_usage();

    let measured = session
        .measure_context_usage(
            vec![crate::tests::system("be brief")],
            PromptOptions::default(),
            Cancellation::new(),
        )
        .await
        .expect("measuring a system message is not a conversation change");

    assert!(measured > 0);
    assert_eq!(session.context_usage(), usage, "measurement does not mutate state");
    let error = session
        .append(vec![crate::tests::system("be brief")], Cancellation::new())
        .await
        .expect_err("committing one still follows the conversation rule");
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
}

#[tokio::test]
async fn measurement_accounts_for_the_constraint_input_choice() {
    let (_engine, session) = scripted_session().await;
    let constraint = json!({
        "type": "object",
        "properties": { "animal": { "type": "string" } },
        "required": ["animal"],
        "additionalProperties": false
    });

    let included = session
        .measure_context_usage(
            vec![user("choose")],
            PromptOptions {
                response_constraint: Some(constraint.clone()),
                omit_response_constraint_input: false,
            },
            Cancellation::new(),
        )
        .await
        .expect("measurement with the schema input");
    let omitted = session
        .measure_context_usage(
            vec![user("choose")],
            PromptOptions {
                response_constraint: Some(constraint.clone()),
                omit_response_constraint_input: true,
            },
            Cancellation::new(),
        )
        .await
        .expect("measurement without the schema input");

    assert_eq!(included - omitted, schema_tokens(&constraint));
    assert_eq!(
        omitted,
        transcript_tokens(&[user("choose")]) - transcript_tokens(&[]),
        "suppressing the schema input also removes it from the accounting"
    );
}

#[tokio::test]
async fn oversized_initial_history_fails_before_a_session_exists() {
    let engine = ScriptedEngine::new();
    engine.set_context_window(6);
    let options = CreateOptions {
        initial_prompts: vec![user("one two three four five six seven eight")],
        ..CreateOptions::default()
    };

    let error = Session::create_with(engine.engine(), options, Cancellation::new())
        .await
        .expect_err("oversized initialization fails");

    assert_eq!(error.kind(), ErrorKind::QuotaExceeded);
    assert!(
        !engine.log().iter().any(|event| matches!(event, Event::Opened(_))),
        "no native session is opened for an oversized conversation"
    );
}

#[tokio::test]
async fn oversized_prefill_fails_without_committing_or_generating() {
    let engine = ScriptedEngine::new();
    engine.set_context_window(12);
    let session =
        Session::create_with(engine.engine(), CreateOptions::default(), Cancellation::new())
            .await
            .expect("session");
    session.append(vec![user("one two three")], Cancellation::new()).await.expect("append");
    let usage = session.context_usage();

    let error = session
        .prompt(
            vec![user("four five six seven eight nine ten eleven twelve")],
            PromptOptions::default(),
            Cancellation::new(),
        )
        .await
        .expect_err("oversized prefill fails");

    assert_eq!(error.kind(), ErrorKind::QuotaExceeded);
    assert_eq!(session.context_usage(), usage, "the rejected input is not committed");
    assert!(engine.requests().is_empty(), "no generation is started");

    engine.script(Script::text(["ok"]));
    session
        .prompt(vec![user("four")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("the session remains usable");
    assert_eq!(engine.requests()[0].committed, vec![user("one two three")]);
}

#[tokio::test]
async fn usage_includes_generated_output() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::text(["two words"]));

    session
        .prompt(vec![user("talk")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("prompt resolves");

    assert_eq!(
        session.context_usage(),
        transcript_tokens(&[user("talk"), assistant("two words")]) - EMPTY_OVERHEAD
    );
}

#[tokio::test]
async fn usage_reflects_accepted_input_before_a_stream_finishes() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::text_then_hang(["partial"]));

    let mut generation = session
        .stream(vec![user("one two three")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("stream starts");
    assert_eq!(generation.next().await.expect("chunk").expect("ok"), "partial");

    assert_eq!(
        session.context_usage(),
        transcript_tokens(&[user("one two three")]) - EMPTY_OVERHEAD,
        "accepted input is accounted before the operation settles"
    );

    drop(generation);
}

#[tokio::test]
async fn a_clone_starts_with_the_source_usage_and_capacity() {
    let engine = ScriptedEngine::new();
    engine.set_context_window(64);
    let session =
        Session::create_with(engine.engine(), CreateOptions::default(), Cancellation::new())
            .await
            .expect("session");
    session.append(vec![user("one two three")], Cancellation::new()).await.expect("append");

    let clone = session.fork(Cancellation::new()).await.expect("fork resolves");

    assert_eq!(clone.context_usage(), session.context_usage(), "the clone inherits usage");
    assert_eq!(clone.context_window(), 64 - EMPTY_OVERHEAD, "the clone inherits capacity");
    assert_ne!(clone.context_usage(), 0, "cloning does not reset the quota");
}

#[tokio::test]
async fn a_clone_of_a_full_conversation_cannot_generate_more() {
    let engine = ScriptedEngine::new();
    engine.set_context_window(10);
    let session =
        Session::create_with(engine.engine(), CreateOptions::default(), Cancellation::new())
            .await
            .expect("session");
    session
        .append(vec![user("one two three four five six")], Cancellation::new())
        .await
        .expect("append");

    let clone = session.fork(Cancellation::new()).await.expect("fork resolves");
    let error = clone
        .prompt(vec![user("seven eight nine ten")], PromptOptions::default(), Cancellation::new())
        .await
        .expect_err("a clone does not create capacity");

    assert_eq!(error.kind(), ErrorKind::QuotaExceeded);
}

#[tokio::test]
async fn a_generated_response_that_exceeds_the_window_fails_without_committing_it() {
    let engine = ScriptedEngine::new();
    engine.set_context_window(12);
    let session =
        Session::create_with(engine.engine(), CreateOptions::default(), Cancellation::new())
            .await
            .expect("session");
    engine.script(Script::text(["one two three four five six seven eight nine ten"]));

    let error = session
        .prompt(vec![user("talk")], PromptOptions::default(), Cancellation::new())
        .await
        .expect_err("generation that exhausts the window fails");

    assert_eq!(error.kind(), ErrorKind::QuotaExceeded);
    assert_eq!(
        session.context_usage(),
        transcript_tokens(&[user("talk")]) - EMPTY_OVERHEAD,
        "the accepted input is retained and the response is not committed"
    );

    engine.script(Script::text(["ok"]));
    session
        .prompt(vec![user("again")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("the session remains usable");
    assert_eq!(engine.requests()[1].committed, vec![user("talk")]);
}

#[tokio::test]
async fn a_constrained_turn_keeps_its_response_format_in_the_accounting() {
    let (engine, session) = scripted_session().await;
    let constraint = json!({
        "type": "object",
        "properties": { "animal": { "type": "string" } },
        "required": ["animal"],
        "additionalProperties": false
    });
    engine.script(Script::structured(["{\"animal\":\"crab\"}"]));

    session
        .prompt(
            vec![user("choose")],
            PromptOptions {
                response_constraint: Some(constraint.clone()),
                omit_response_constraint_input: false,
            },
            Cancellation::new(),
        )
        .await
        .expect("constrained generation resolves");

    let expected = transcript_tokens(&[user("choose"), assistant("{\"animal\":\"crab\"}")])
        - EMPTY_OVERHEAD
        + schema_tokens(&constraint);
    assert_eq!(
        session.context_usage(),
        expected,
        "the response format stays part of the committed conversation"
    );

    let clone = session.fork(Cancellation::new()).await.expect("fork resolves");
    assert_eq!(
        clone.context_usage(),
        expected,
        "a clone inherits the constrained turn's representation"
    );

    engine.script(Script::text(["after"]));
    session
        .prompt(vec![user("next")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("the next turn still fits");
    assert_eq!(
        session.context_usage(),
        transcript_tokens(&[
            user("choose"),
            assistant("{\"animal\":\"crab\"}"),
            user("next"),
            assistant("after"),
        ]) - EMPTY_OVERHEAD
            + schema_tokens(&constraint),
        "an unconstrained turn does not drop the earlier response format"
    );
}
