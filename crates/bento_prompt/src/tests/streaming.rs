//! Streaming deltas, prefix rewrites, cancellation, retention and settlement.

use std::time::{Duration, Instant};

use serde_json::json;

use crate::session::GENERATION_BUFFER;
use crate::tests::scripted::{EMPTY_OVERHEAD, Event, Script, transcript_tokens};
use crate::tests::{assistant, position, scripted_session, user};
use crate::{Cancellation, ErrorKind, Message, PromptOptions};

async fn drain(generation: &mut crate::Generation) -> Vec<Result<String, ErrorKind>> {
    let mut chunks = Vec::new();
    while let Some(chunk) = generation.next().await {
        chunks.push(chunk.map_err(|error| error.kind()));
    }
    chunks
}

/// Native token counts the engine has started.
fn counts(events: &[Event]) -> usize {
    events.iter().filter(|event| matches!(event, Event::Counting)).count()
}

#[tokio::test]
async fn text_stream_emits_incremental_deltas() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::text(["He", "Hello", "Hello there"]));

    let mut generation = session
        .stream(vec![user("greet me")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("stream starts");
    let chunks = drain(&mut generation).await;

    let text: Vec<String> = chunks.into_iter().map(|chunk| chunk.expect("chunk")).collect();
    assert_eq!(text, vec!["He", "llo", " there"]);
    assert_eq!(text.concat(), "Hello there", "chunks concatenate to the response");
}

#[tokio::test]
async fn completed_stream_commits_the_assistant_turn() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::text(["Hel", "Hello"]));

    let mut generation = session
        .stream(vec![user("greet me")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("stream starts");
    drain(&mut generation).await;

    engine.script(Script::text(["again"]));
    session
        .prompt(vec![user("again")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("second turn");

    assert_eq!(
        engine.requests()[1].committed,
        vec![user("greet me"), assistant("Hello")],
        "the generated response is committed as an assistant turn"
    );
}

#[tokio::test]
async fn rewriting_an_emitted_prefix_fails_the_stream() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::text(["Hello", "Goodbye"]));

    let mut generation = session
        .stream(vec![user("greet me")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("stream starts");

    assert_eq!(generation.next().await.expect("first chunk").expect("ok"), "Hello");
    let error = generation.next().await.expect("terminal").expect_err("rewrite fails");
    assert_eq!(error.kind(), ErrorKind::Unknown);
    assert!(generation.next().await.is_none(), "the stream is finished");

    engine.wait_for(|events| events.iter().any(|event| matches!(event, Event::Settled(0)))).await;
}

#[tokio::test]
async fn a_rewrite_shorter_than_the_delivered_text_also_fails_the_stream() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::text(["Hello there", "Hi"]));

    let mut generation = session
        .stream(vec![user("greet me")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("stream starts");

    assert_eq!(generation.next().await.expect("first chunk").expect("ok"), "Hello there");
    let error = generation.next().await.expect("terminal").expect_err("rewrite fails");
    assert_eq!(error.kind(), ErrorKind::Unknown);

    engine.script(Script::text(["recovered"]));
    session
        .prompt(vec![user("again")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("the session is reusable after a rewritten response");
    assert_eq!(
        engine.requests()[1].committed,
        vec![user("greet me")],
        "a rewritten response commits no assistant turn"
    );
}

#[tokio::test]
async fn prompt_returns_the_whole_response() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::text(["He", "Hello", "Hello there"]));

    let answer = session
        .prompt(vec![user("greet me")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("prompt resolves");

    assert_eq!(answer, "Hello there");
}

#[tokio::test]
async fn cancelled_generation_retains_input_and_settles_before_the_next_operation() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::text_then_hang(["He"]));
    let cancel = Cancellation::new();

    let mut generation = session
        .stream(vec![user("greet me")], PromptOptions::default(), cancel.clone())
        .await
        .expect("stream starts");
    assert_eq!(generation.next().await.expect("chunk").expect("ok"), "He");

    cancel.cancel();
    let error = generation.next().await.expect("terminal").expect_err("cancelled");
    assert_eq!(error.kind(), ErrorKind::Aborted);
    assert!(generation.next().await.is_none());

    engine.script(Script::text(["second"]));
    session
        .prompt(vec![user("again")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("the session is reusable");

    assert_eq!(
        engine.requests()[1].committed,
        vec![user("greet me")],
        "the accepted input is retained and the partial response is discarded"
    );
    let events = engine.log();
    let cancelled = position(&events, |event| matches!(event, Event::Cancelled(0)));
    let settled = position(&events, |event| matches!(event, Event::Settled(0)));
    let restored = position(
        &events,
        |event| matches!(event, Event::Restored(history) if history == &vec![user("greet me")]),
    );
    let generated = position(&events, |event| matches!(event, Event::Generated(1)));
    assert!(cancelled < settled, "cancellation precedes settlement");
    assert!(settled < restored, "the transcript is restored after settlement");
    assert!(restored < generated, "the next generation runs on the restored transcript");
}

#[tokio::test]
async fn dropping_a_generation_cancels_and_settles_before_the_next_operation() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::hang());

    let generation = session
        .stream(vec![user("greet me")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("stream starts");
    engine.wait_for(|events| events.iter().any(|event| matches!(event, Event::Generated(0)))).await;
    drop(generation);

    engine.script(Script::text(["second"]));
    session
        .prompt(vec![user("again")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("the session is reusable after a dropped stream");

    let events = engine.log();
    let cancelled = position(&events, |event| matches!(event, Event::Cancelled(0)));
    let settled = position(&events, |event| matches!(event, Event::Settled(0)));
    let generated = position(&events, |event| matches!(event, Event::Generated(1)));
    assert!(cancelled < settled && settled < generated);
}

#[tokio::test]
async fn transport_stays_bounded_when_the_consumer_stops_reading() {
    let (engine, session) = scripted_session().await;
    let snapshots: Vec<String> = (1..=20).map(|length| "x".repeat(length)).collect();
    engine.script(Script::text(snapshots));

    let generation = session
        .stream(vec![user("talk")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("stream starts");

    // Let the session pull as much as it is willing to buffer, then confirm it
    // stopped: the consumer never read a chunk.
    let deadline = Instant::now() + Duration::from_secs(2);
    while engine.consumed_steps() < GENERATION_BUFFER && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(
        engine.consumed_steps() <= GENERATION_BUFFER + 1,
        "an unread stream buffered {} snapshots, bound is {}",
        engine.consumed_steps(),
        GENERATION_BUFFER + 1
    );

    drop(generation);
    engine.wait_for(|events| events.iter().any(|event| matches!(event, Event::Settled(0)))).await;
}

#[tokio::test]
async fn generation_failure_keeps_delivered_chunks_and_accepted_input() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::text_then_fail(["partial"], ErrorKind::QuotaExceeded));

    let mut generation = session
        .stream(vec![user("talk")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("stream starts");

    assert_eq!(generation.next().await.expect("chunk").expect("ok"), "partial");
    let error = generation.next().await.expect("terminal").expect_err("quota failure");
    assert_eq!(error.kind(), ErrorKind::QuotaExceeded);

    engine.script(Script::text(["next"]));
    session
        .prompt(vec![user("again")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("session remains usable");
    assert_eq!(engine.requests()[1].committed, vec![user("talk")]);
}

#[tokio::test]
async fn refusal_is_reported_as_not_readable() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::failing(ErrorKind::NotReadable));

    let error = session
        .prompt(vec![user("talk")], PromptOptions::default(), Cancellation::new())
        .await
        .expect_err("refusal fails the prompt");

    assert_eq!(error.kind(), ErrorKind::NotReadable);
}

#[tokio::test]
async fn constrained_stream_emits_one_validated_json_result() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::structured(["{}", "{\"animal\":", "{\"animal\":\"crab\"}"]));
    let options = PromptOptions {
        response_constraint: Some(json!({
            "type": "object",
            "properties": { "animal": { "type": "string", "enum": ["crab", "snail"] } },
            "required": ["animal"],
            "additionalProperties": false,
        })),
        omit_response_constraint_input: false,
    };

    let mut generation = session
        .stream(vec![user("choose")], options, Cancellation::new())
        .await
        .expect("stream starts");

    let first = generation.next().await.expect("result").expect("valid json");
    assert_eq!(first, "{\"animal\":\"crab\"}");
    assert!(generation.next().await.is_none(), "constrained streams emit one result");
}

#[tokio::test]
async fn already_cancelled_operation_fails_without_generating() {
    let (engine, session) = scripted_session().await;
    let cancel = Cancellation::new();
    cancel.cancel();

    let error = session
        .prompt(vec![user("talk")], PromptOptions::default(), cancel)
        .await
        .expect_err("an already-cancelled operation fails");

    assert_eq!(error.kind(), ErrorKind::Aborted);
    assert!(engine.requests().is_empty(), "no generation is started");

    engine.script(Script::text(["fresh"]));
    session
        .prompt(vec![user("talk")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("the session is unaffected");
    let committed: Vec<Message> = engine.requests()[0].committed.clone();
    assert!(committed.is_empty(), "a cancelled operation commits nothing");
}

#[tokio::test]
async fn cancelling_an_operation_supersedes_buffered_chunks() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::text_then_hang(["a", "ab", "abc", "abcd", "abcde"]));
    let cancellation = Cancellation::new();

    let mut generation = session
        .stream(vec![user("talk")], PromptOptions::default(), cancellation.clone())
        .await
        .expect("stream starts");

    // Fill the transport while the generation is still running.
    let deadline = Instant::now() + Duration::from_secs(2);
    while engine.consumed_steps() < GENERATION_BUFFER && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(engine.consumed_steps() > 0, "the generation produced chunks to buffer");

    cancellation.cancel();

    let error = generation
        .next()
        .await
        .expect("a cancelled generation reports its terminal")
        .expect_err("cancellation supersedes chunks that were already buffered");
    assert_eq!(error.kind(), ErrorKind::Aborted);
    assert!(generation.next().await.is_none(), "the generation is finished after its terminal");
}

#[tokio::test]
async fn a_cancelled_reader_closes_without_inventing_an_error() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::text_then_hang(["He", "Hello"]));

    let mut generation = session
        .stream(vec![user("talk")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("stream starts");
    assert_eq!(generation.next().await.expect("chunk").expect("ok"), "He");

    generation.cancel();

    assert!(
        generation.next().await.is_none(),
        "a consumer that cancels its own read gets no fabricated error"
    );
    engine.wait_for(|events| events.iter().any(|event| matches!(event, Event::Settled(0)))).await;
}

#[tokio::test]
async fn aborting_while_the_final_count_is_held_commits_no_response() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::text(["Hello there"]));
    engine.script(Script::text(["second"]));
    let before = counts(&engine.log());
    // The preflight count runs; the count that decides whether the response is
    // committed is still in flight when the operation is aborted.
    let hold = engine.hold_counts_after(1);
    let cancellation = Cancellation::new();

    let prompt =
        session.prompt(vec![user("greet me")], PromptOptions::default(), cancellation.clone());
    engine.wait_for(|events| counts(events) > before + 1).await;
    // Work queued while the model is still counting must run on the state the
    // abandoned turn leaves behind.
    let queued = session.prompt(vec![user("again")], PromptOptions::default(), Cancellation::new());
    cancellation.cancel();
    hold.release();

    let error = prompt.await.expect_err("a turn aborted at its final count fails");
    assert_eq!(error.kind(), ErrorKind::Aborted);
    assert_eq!(queued.await.expect("the queued turn runs"), "second");

    assert_eq!(
        engine.requests()[1].committed,
        vec![user("greet me")],
        "the accepted input is retained and the unreceived response is not committed"
    );
    let committed = vec![user("greet me"), user("again"), assistant("second")];
    assert_eq!(
        session.context_usage(),
        transcript_tokens(&committed) - EMPTY_OVERHEAD,
        "usage accounts for the committed turns only"
    );
    let events = engine.log();
    let restored = position(
        &events,
        |event| matches!(event, Event::Restored(history) if history == &vec![user("greet me")]),
    );
    let generated = position(&events, |event| matches!(event, Event::Generated(1)));
    assert!(restored < generated, "the queued turn generates on the restored transcript");
}

#[tokio::test]
async fn a_reader_cancelled_at_the_final_count_closes_and_commits_nothing() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::text(["Hello there"]));
    let before = counts(&engine.log());
    let hold = engine.hold_counts_after(1);

    let mut generation = session
        .stream(vec![user("greet me")], PromptOptions::default(), Cancellation::new())
        .await
        .expect("stream starts");
    engine.wait_for(|events| counts(events) > before + 1).await;
    generation.cancel();
    hold.release();

    assert!(
        generation.next().await.is_none(),
        "a reader that stopped reading gets no fabricated error"
    );

    engine.script(Script::text(["second"]));
    assert_eq!(
        session
            .prompt(vec![user("again")], PromptOptions::default(), Cancellation::new())
            .await
            .expect("the session is reusable"),
        "second"
    );
    assert_eq!(
        engine.requests()[1].committed,
        vec![user("greet me")],
        "the response the reader never received is not committed"
    );
    let committed = vec![user("greet me"), user("again"), assistant("second")];
    assert_eq!(
        session.context_usage(),
        transcript_tokens(&committed) - EMPTY_OVERHEAD,
        "usage accounts for the committed turns only"
    );
}
