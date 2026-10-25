//! A Rust consumer of `bento_prompt` with the native backend, used as the
//! live preview for proposal.0002.
//!
//! Run it on an eligible Mac:
//!
//! ```sh
//! MACOSX_DEPLOYMENT_TARGET=27.0 cargo run -p bento_prompt --features foundation-models --example prompt_session
//! ```
//!
//! It asserts the behavior the proposal promises and exits non-zero on the
//! first failure. Genuine unavailability is a failure here, not a silent pass.

use std::process::ExitCode;

use bento_prompt::{
    Availability, Cancellation, CreateOptions, ErrorKind, Expected, Message, PromptOptions, Role,
    Session,
};
use serde_json::{Value, json};

fn user(content: &str) -> Message {
    Message { role: Role::User, content: content.to_owned() }
}

fn assistant(content: &str) -> Message {
    Message { role: Role::Assistant, content: content.to_owned() }
}

fn system(content: &str) -> Message {
    Message { role: Role::System, content: content.to_owned() }
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => {
            println!("prompt_session: every assertion passed");
            ExitCode::SUCCESS
        }
        Err(failure) => {
            eprintln!("prompt_session: {failure}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), String> {
    let english = CreateOptions {
        expected_inputs: vec![Expected {
            kind: "text".to_owned(),
            languages: vec!["en".to_owned()],
        }],
        expected_outputs: vec![Expected {
            kind: "text".to_owned(),
            languages: vec!["en".to_owned()],
        }],
        ..CreateOptions::default()
    };

    let availability =
        bento_prompt::availability(&english).await.map_err(|error| error.to_string())?;
    if availability != Availability::Available {
        return Err(format!(
            "the on-device model is {availability:?}; this preview needs Apple Intelligence \
             enabled, the model ready, and macOS {} or newer",
            bento_prompt::MACOS_DEPLOYMENT_FLOOR
        ));
    }
    println!("sdk: {:?}", bento_prompt::NATIVE_SDK_VERSION);

    empty_session_accounting().await?;
    conversation_and_streaming().await?;
    constrained_json().await?;
    clone_and_cancellation().await?;
    Ok(())
}

/// A session with no history starts at zero usage; instructions cost context.
async fn empty_session_accounting() -> Result<(), String> {
    let empty = Session::create(CreateOptions::default(), Cancellation::new())
        .await
        .map_err(|error| format!("creating an empty session failed: {error}"))?;
    if empty.context_usage() != 0 {
        return Err(format!("an empty session reported {} tokens", empty.context_usage()));
    }
    if empty.context_window() == 0 {
        return Err("the model reported no usable context window".to_owned());
    }
    let window = empty.context_window();
    empty.dispose().await;

    let instructed = Session::create(
        CreateOptions {
            initial_prompts: vec![system("Answer in one short sentence.")],
            ..CreateOptions::default()
        },
        Cancellation::new(),
    )
    .await
    .map_err(|error| format!("creating an instructed session failed: {error}"))?;
    if instructed.context_usage() == 0 {
        return Err("initial instructions were not accounted for".to_owned());
    }
    if instructed.context_window() != window {
        return Err("sessions disagreed about the usable context window".to_owned());
    }
    instructed.dispose().await;
    Ok(())
}

/// Multi-turn recall, append without inference, measurement, and genuine
/// incremental streaming.
async fn conversation_and_streaming() -> Result<(), String> {
    let session = Session::create(
        CreateOptions {
            initial_prompts: vec![system("Answer in one short sentence.")],
            ..CreateOptions::default()
        },
        Cancellation::new(),
    )
    .await
    .map_err(|error| format!("creating a session failed: {error}"))?;

    let before_append = session.context_usage();
    session
        .append(
            vec![user("Remember the code word: kelp."), assistant("The code word is kelp.")],
            Cancellation::new(),
        )
        .await
        .map_err(|error| format!("append failed: {error}"))?;
    let after_append = session.context_usage();
    if after_append <= before_append {
        return Err("append did not account for the committed messages".to_owned());
    }

    let measured = session
        .measure_context_usage(
            vec![user("What is the code word?")],
            PromptOptions::default(),
            Cancellation::new(),
        )
        .await
        .map_err(|error| format!("measurement failed: {error}"))?;
    if measured == 0 {
        return Err("measuring a non-empty prompt reported zero tokens".to_owned());
    }
    if session.context_usage() != after_append {
        return Err("measurement changed the committed conversation".to_owned());
    }

    let answer = session
        .prompt(vec![user("What is the code word?")], PromptOptions::default(), Cancellation::new())
        .await
        .map_err(|error| format!("generation failed: {error}"))?;
    if !answer.to_lowercase().contains("kelp") {
        return Err(format!("the model did not recall the appended turn: {answer:?}"));
    }
    if session.context_usage() <= after_append {
        return Err("the generated turn was not accounted for".to_owned());
    }

    let mut generation = session
        .stream(
            vec![user("Name three tide-pool animals.")],
            PromptOptions::default(),
            Cancellation::new(),
        )
        .await
        .map_err(|error| format!("streaming failed to start: {error}"))?;
    let mut chunks: Vec<String> = Vec::new();
    while let Some(chunk) = generation.next().await {
        chunks.push(chunk.map_err(|error| format!("streaming failed: {error}"))?);
    }
    if chunks.is_empty() {
        return Err("the stream delivered no chunks".to_owned());
    }
    if chunks.iter().any(String::is_empty) {
        return Err("the stream delivered an empty chunk".to_owned());
    }
    let streamed = chunks.concat();
    if streamed.trim().is_empty() {
        return Err("the streamed chunks concatenated to nothing".to_owned());
    }
    println!("streamed {} chunk(s), {} characters", chunks.len(), streamed.len());

    session.dispose().await;
    Ok(())
}

/// Constrained generation across the supported subset: nested object, array
/// bounds, string enum, integer and number bounds, boolean, optional property.
async fn constrained_json() -> Result<(), String> {
    let session = Session::create(CreateOptions::default(), Cancellation::new())
        .await
        .map_err(|error| format!("creating a session failed: {error}"))?;

    let constraint = json!({
        "type": "object",
        "title": "TidePoolSurvey",
        "properties": {
            "habitat": { "type": "string", "description": "Where the survey took place" },
            "protected": { "type": "boolean" },
            "salinity": { "type": "number", "minimum": 20.0, "maximum": 40.0 },
            "note": { "type": "string", "description": "Optional remark" },
            "animals": {
                "type": "array",
                "minItems": 2,
                "maxItems": 3,
                "items": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "enum": ["crab", "anemone", "snail"] },
                        "count": { "type": "integer", "minimum": 1, "maximum": 20 }
                    },
                    "required": ["name", "count"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["habitat", "protected", "salinity", "animals"],
        "additionalProperties": false
    });

    let text = session
        .prompt(
            vec![user("Report one tide-pool survey.")],
            PromptOptions {
                response_constraint: Some(constraint),
                omit_response_constraint_input: false,
            },
            Cancellation::new(),
        )
        .await
        .map_err(|error| format!("constrained generation failed: {error}"))?;

    let value: Value =
        serde_json::from_str(&text).map_err(|error| format!("result is not JSON: {error}"))?;
    let object = value.as_object().ok_or_else(|| "result is not an object".to_owned())?;

    for required in ["habitat", "protected", "salinity", "animals"] {
        if !object.contains_key(required) {
            return Err(format!("result is missing `{required}`: {text}"));
        }
    }
    if object.keys().any(|key| {
        !matches!(key.as_str(), "habitat" | "protected" | "salinity" | "note" | "animals")
    }) {
        return Err(format!("result declared an undeclared property: {text}"));
    }
    if let Some(note) = object.get("note")
        && !note.is_string()
    {
        return Err(format!(
            "an omitted optional property must be absent, not null or another type: {text}"
        ));
    }
    if !object["protected"].is_boolean() {
        return Err(format!("`protected` is not a boolean: {text}"));
    }
    let salinity =
        object["salinity"].as_f64().ok_or_else(|| format!("`salinity` is not a number: {text}"))?;
    if !(20.0..=40.0).contains(&salinity) {
        return Err(format!("`salinity` is outside its declared bounds: {text}"));
    }
    let animals =
        object["animals"].as_array().ok_or_else(|| format!("`animals` is not an array: {text}"))?;
    if !(2..=3).contains(&animals.len()) {
        return Err(format!("`animals` violates its length bounds: {text}"));
    }
    for animal in animals {
        let name = animal["name"].as_str().unwrap_or_default();
        if !matches!(name, "crab" | "anemone" | "snail") {
            return Err(format!("`name` is outside its enumeration: {text}"));
        }
        let count =
            animal["count"].as_i64().ok_or_else(|| format!("`count` is not an integer: {text}"))?;
        if !(1..=20).contains(&count) {
            return Err(format!("`count` is outside its declared bounds: {text}"));
        }
    }
    println!("constrained result: {text}");

    session.dispose().await;
    Ok(())
}

/// Independent clones, cancellation during streaming, and awaited disposal.
async fn clone_and_cancellation() -> Result<(), String> {
    let session = Session::create(CreateOptions::default(), Cancellation::new())
        .await
        .map_err(|error| format!("creating a session failed: {error}"))?;
    session
        .append(vec![user("Remember the code word: kelp.")], Cancellation::new())
        .await
        .map_err(|error| format!("append failed: {error}"))?;

    let clone = session
        .fork(Cancellation::new())
        .await
        .map_err(|error| format!("cloning failed: {error}"))?;
    if clone.context_usage() != session.context_usage() {
        return Err("the clone did not inherit the source's context usage".to_owned());
    }

    let cancellation = Cancellation::new();
    let mut generation = session
        .stream(
            vec![user("Describe a tide pool in detail.")],
            PromptOptions::default(),
            cancellation.clone(),
        )
        .await
        .map_err(|error| format!("streaming failed to start: {error}"))?;
    let first = generation
        .next()
        .await
        .ok_or_else(|| "the stream ended before any chunk".to_owned())?
        .map_err(|error| format!("streaming failed: {error}"))?;
    if first.is_empty() {
        return Err("the first streamed chunk was empty".to_owned());
    }
    cancellation.cancel();
    match generation.next().await {
        Some(Err(error)) if error.kind() == ErrorKind::Aborted => {}
        Some(Err(error)) => return Err(format!("cancellation reported {error}")),
        Some(Ok(chunk)) => return Err(format!("a cancelled stream delivered {chunk:?}")),
        None => return Err("a cancelled stream ended without reporting cancellation".to_owned()),
    }
    drop(generation);

    // The cancelled request must have settled before the session is reused.
    let reused = session
        .prompt(vec![user("Say OK.")], PromptOptions::default(), Cancellation::new())
        .await
        .map_err(|error| format!("the session was unusable after cancellation: {error}"))?;
    if reused.trim().is_empty() {
        return Err("the reused session produced an empty response".to_owned());
    }

    session.destroy();
    session.dispose().await;
    let after_disposal = clone
        .prompt(vec![user("What is the code word?")], PromptOptions::default(), Cancellation::new())
        .await
        .map_err(|error| format!("the clone did not survive its source: {error}"))?;
    if !after_disposal.to_lowercase().contains("kelp") {
        return Err(format!("the clone lost the source's history: {after_disposal:?}"));
    }

    let destroyed = session
        .prompt(vec![user("Still there?")], PromptOptions::default(), Cancellation::new())
        .await;
    match destroyed {
        Err(error) if error.kind() == ErrorKind::Aborted => {}
        Err(error) => return Err(format!("a destroyed session reported {error}")),
        Ok(_) => return Err("a destroyed session generated a response".to_owned()),
    }

    clone.dispose().await;
    Ok(())
}
