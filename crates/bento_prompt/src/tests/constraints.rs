//! The closed JSON Schema subset: translation, rejection and output validation.

use serde_json::{Value, json};

use crate::schema::Schema;
use crate::tests::scripted::Script;
use crate::tests::{scripted_session, user};
use crate::{Cancellation, ErrorKind, PromptOptions};

fn translate(source: Value) -> Result<Schema, crate::Error> {
    Schema::translate(&source)
}

#[test]
fn fractional_bounds_on_an_integer_schema_narrow_to_permitted_integers() {
    let schema = translate(json!({ "type": "integer", "minimum": 0.5, "maximum": 9.5 }))
        .expect("fractional bounds are a supported numeric constraint");

    schema.validate(&json!(1)).expect("the narrowed minimum is permitted");
    schema.validate(&json!(9)).expect("the narrowed maximum is permitted");
    schema.validate(&json!(0)).expect_err("below the narrowed minimum");
    schema.validate(&json!(10)).expect_err("above the narrowed maximum");

    let error = translate(json!({ "type": "integer", "minimum": 2.2, "maximum": 2.8 }))
        .expect_err("bounds that permit no integer are malformed");
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
}

#[test]
fn unsupported_schema_features_are_rejected_as_unsupported() {
    let cases = [
        json!({ "$ref": "#/definitions/other" }),
        json!({ "type": "object", "properties": { "a": { "$ref": "#/x" } } }),
        json!({ "oneOf": [{ "type": "string" }, { "type": "integer" }] }),
        json!({ "anyOf": [{ "type": "string" }] }),
        json!({ "allOf": [{ "type": "object" }] }),
        json!({ "not": { "type": "string" } }),
        json!({ "type": "null" }),
        json!({ "type": ["string", "null"] }),
        json!({ "type": "string", "pattern": "^crab$" }),
        json!({ "type": "string", "format": "email" }),
        json!({ "type": "integer", "exclusiveMinimum": 1 }),
        json!({ "type": "object", "additionalProperties": true }),
        json!({ "type": "object", "additionalProperties": { "type": "string" } }),
        json!({ "type": "object", "patternProperties": { "^a": { "type": "string" } } }),
        json!({ "type": "array", "items": [{ "type": "string" }] }),
        json!({ "type": "array", "uniqueItems": true }),
        json!({ "type": "widget" }),
        json!({ "type": "string", "enum": [1, 2] }),
    ];

    for case in cases {
        let error = translate(case.clone()).expect_err("unsupported schema rejected");
        assert_eq!(error.kind(), ErrorKind::NotSupported, "schema {case} must be unsupported");
    }
}

#[test]
fn malformed_supported_keywords_are_invalid_input() {
    let cases = [
        json!(true),
        json!({}),
        json!({ "type": "object", "required": "habitat" }),
        json!({ "type": "object", "required": ["missing"] }),
        json!({ "type": "object", "properties": [] }),
        json!({ "type": "object", "additionalProperties": 5 }),
        json!({ "type": "array", "items": { "type": "string" }, "minItems": -1 }),
        json!({ "type": "array", "items": { "type": "string" }, "maxItems": "3" }),
        json!({ "type": "array", "items": { "type": "string" }, "minItems": 3, "maxItems": 1 }),
        json!({ "type": "array" }),
        json!({ "type": "string", "enum": "crab" }),
        json!({ "type": "string", "enum": [] }),
        json!({ "type": "integer", "minimum": "1" }),
        json!({ "type": "number", "minimum": 5, "maximum": 1 }),
        json!({ "type": "string", "title": 7 }),
    ];

    for case in cases {
        let error = translate(case.clone()).expect_err("malformed schema rejected");
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "schema {case} must be malformed");
    }
}

#[test]
fn duplicate_required_names_are_invalid_input() {
    let error = translate(json!({
        "type": "object",
        "properties": { "habitat": { "type": "string" } },
        "required": ["habitat", "habitat"],
        "additionalProperties": false
    }))
    .expect_err("`required` must not repeat a property name");
    assert_eq!(error.kind(), ErrorKind::InvalidInput);

    translate(json!({
        "type": "object",
        "properties": { "habitat": { "type": "string" } },
        "required": ["habitat"],
        "additionalProperties": false
    }))
    .expect("naming each required property once stays supported");
}

#[test]
fn integral_floating_output_satisfies_an_integer_schema() {
    let bounded = translate(json!({ "type": "integer", "minimum": -2, "maximum": 10 }))
        .expect("supported integer schema");

    bounded.validate(&json!(1.0)).expect("a zero fractional part is an integer");
    bounded.validate(&json!(-2.0)).expect("an integral float at the minimum is permitted");
    bounded.validate(&json!(10.0)).expect("an integral float at the maximum is permitted");

    let violations = [json!(1.5), json!(-2.5), json!(-3.0), json!(11.0)];
    for violation in violations {
        let error = bounded.validate(&violation).expect_err("violation rejected");
        assert_eq!(error.kind(), ErrorKind::Unknown, "value {violation} must fail validation");
    }

    let unbounded = translate(json!({ "type": "integer" })).expect("supported integer schema");
    unbounded.validate(&json!(-9007199254740993_i64)).expect("a large negative integer");
    unbounded.validate(&json!(0.5)).expect_err("a fractional value is not an integer");
    unbounded.validate(&json!(1e30)).expect_err("outside the supported integer range");
}

#[test]
fn integral_floating_item_counts_are_accepted_and_other_counts_are_invalid() {
    let schema = translate(
        json!({ "type": "array", "items": { "type": "string" }, "minItems": 1.0, "maxItems": 2.0 }),
    )
    .expect("item counts with a zero fractional part are integers");

    schema.validate(&json!(["a"])).expect("one item meets `minItems`");
    schema.validate(&json!(["a", "b"])).expect("two items meet `maxItems`");
    schema.validate(&json!([])).expect_err("fewer items than `minItems`");
    schema.validate(&json!(["a", "b", "c"])).expect_err("more items than `maxItems`");

    let malformed = [
        json!({ "type": "array", "items": { "type": "string" }, "minItems": -1.0 }),
        json!({ "type": "array", "items": { "type": "string" }, "minItems": 1.5 }),
        json!({ "type": "array", "items": { "type": "string" }, "maxItems": 2.5 }),
        json!({ "type": "array", "items": { "type": "string" }, "minItems": 2.0, "maxItems": 1.0 }),
    ];
    for case in malformed {
        let error = translate(case.clone()).expect_err("malformed item count rejected");
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "schema {case} must be malformed");
    }
}

#[test]
fn integer_bounds_and_output_keep_exact_precision_beyond_the_double_range() {
    let schema = translate(json!({
        "type": "integer",
        "minimum": 9007199254740993_i64,
        "maximum": 9223372036854775807_i64
    }))
    .expect("exact integer bounds are supported");

    schema.validate(&json!(9007199254740993_i64)).expect("the exact minimum is permitted");
    schema.validate(&json!(9223372036854775807_i64)).expect("the exact maximum is permitted");
    schema
        .validate(&json!(9007199254740992_i64))
        .expect_err("one below the exact minimum is rejected");
}

#[test]
fn keywords_that_do_not_apply_to_the_declared_type_are_unsupported() {
    let cases = [
        json!({ "type": "integer", "enum": [1, 2] }),
        json!({ "type": "integer", "enum": ["one"] }),
        json!({ "type": "string", "minItems": 1 }),
        json!({ "type": "boolean", "minimum": 0 }),
        json!({ "type": "number", "items": { "type": "string" } }),
        json!({ "type": "object", "properties": {}, "maxItems": 2 }),
        json!({ "type": "array", "items": { "type": "string" }, "required": ["a"] }),
    ];

    for case in cases {
        let error = translate(case.clone()).expect_err("an inapplicable keyword is rejected");
        assert_eq!(error.kind(), ErrorKind::NotSupported, "schema {case} must be unsupported");
    }
}

#[test]
fn bounds_and_item_counts_beyond_the_signed_64_bit_range_are_unsupported() {
    let above_signed = 9223372036854775808_u64;
    let unsigned_limit = 18446744073709551615_u64;
    let cases = [
        json!({ "type": "integer", "minimum": 1e30 }),
        json!({ "type": "integer", "maximum": -1e30 }),
        json!({ "type": "integer", "maximum": unsigned_limit }),
        json!({ "type": "array", "items": { "type": "string" }, "minItems": above_signed }),
        json!({ "type": "array", "items": { "type": "string" }, "maxItems": unsigned_limit }),
        json!({ "type": "array", "items": { "type": "string" }, "maxItems": 1e300 }),
    ];

    for case in cases {
        let error = translate(case.clone()).expect_err("an out-of-range bound is rejected");
        assert_eq!(error.kind(), ErrorKind::NotSupported, "schema {case} must be unsupported");
    }

    let lowest = i64::MIN;
    let highest = i64::MAX;
    translate(json!({ "type": "integer", "minimum": lowest, "maximum": highest }))
        .expect("the signed 64-bit range is a supported integer bound");
    translate(json!({ "type": "array", "items": { "type": "string" }, "maxItems": highest }))
        .expect("the signed 64-bit range is a supported item count");
}

#[test]
fn number_bounds_do_not_round_distinct_integer_outputs_together() {
    for exponent in 53..=62 {
        for sign in [-1_i64, 1] {
            let boundary = (1_i64 << exponent) * sign;
            let schema = translate(json!({
                "type": "number",
                "minimum": boundary,
                "maximum": boundary
            }))
            .expect("a power of two is represented exactly");
            for offset in -3..=3 {
                let candidate = boundary + offset;
                assert_eq!(
                    schema.validate(&json!(candidate)).is_ok(),
                    offset == 0,
                    "{candidate} compared with exact boundary {boundary}"
                );
            }
        }
    }
}

#[test]
fn number_bounds_that_native_floats_cannot_represent_are_unsupported() {
    for bound in [9_007_199_254_740_993_i64, -9_007_199_254_740_993, i64::MAX] {
        for keyword in ["minimum", "maximum"] {
            let mut source = json!({ "type": "number" });
            source[keyword] = json!(bound);
            let error = translate(source).expect_err("a bound must not silently change");
            assert_eq!(error.kind(), ErrorKind::NotSupported);
        }
    }
}

#[test]
fn negative_item_counts_are_malformed_even_outside_native_range() {
    let error = translate(json!({
        "type": "array",
        "items": { "type": "string" },
        "minItems": -1e300
    }))
    .expect_err("negative item counts are malformed");
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
}

#[test]
fn validation_accepts_conforming_output_and_rejects_violations() {
    let schema = translate(json!({
        "type": "object",
        "properties": {
            "habitat": { "type": "string" },
            "animal": { "type": "string", "enum": ["crab", "snail"] },
            "legs": { "type": "integer", "minimum": 2, "maximum": 10 },
            "tags": { "type": "array", "items": { "type": "string" }, "minItems": 1, "maxItems": 2 }
        },
        "required": ["habitat", "animal"],
        "additionalProperties": false
    }))
    .expect("supported schema");

    schema
        .validate(
            &json!({ "habitat": "rock pool", "animal": "crab", "legs": 8, "tags": ["small"] }),
        )
        .expect("a conforming value is accepted");
    schema
        .validate(&json!({ "habitat": "rock pool", "animal": "snail" }))
        .expect("optional properties may be omitted");

    let violations = [
        json!({ "animal": "crab" }),
        json!({ "habitat": "pool", "animal": "otter" }),
        json!({ "habitat": "pool", "animal": "crab", "legs": 1 }),
        json!({ "habitat": "pool", "animal": "crab", "legs": 11 }),
        json!({ "habitat": "pool", "animal": "crab", "legs": 2.5 }),
        json!({ "habitat": "pool", "animal": "crab", "tags": [] }),
        json!({ "habitat": "pool", "animal": "crab", "tags": ["a", "b", "c"] }),
        json!({ "habitat": "pool", "animal": "crab", "tags": [7] }),
        json!({ "habitat": "pool", "animal": "crab", "extra": true }),
        json!({ "habitat": 7, "animal": "crab" }),
        json!(["not an object"]),
    ];
    for violation in violations {
        let error = schema.validate(&violation).expect_err("violation rejected");
        assert_eq!(error.kind(), ErrorKind::Unknown, "value {violation} must fail validation");
    }
}

/// The subset's defaults: an object with no `required` has only optional properties, and an
/// object that does not mention `additionalProperties` is closed all the same.
#[test]
fn omitted_required_and_additional_properties_keep_the_subsets_defaults() {
    let schema = translate(json!({
        "type": "object",
        "properties": { "habitat": { "type": "string" } }
    }))
    .expect("a schema may omit both keywords");

    schema.validate(&json!({})).expect("a property no `required` names may be absent");
    schema.validate(&json!({ "habitat": "rock pool" })).expect("the declared property fits");

    let error = schema
        .validate(&json!({ "habitat": "rock pool", "depth": 2 }))
        .expect_err("an undeclared key is refused by an object that declared no openness");
    assert_eq!(error.kind(), ErrorKind::Unknown);
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 =
            self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, bound: u64) -> usize {
        usize::try_from(self.next() % bound).expect("small bound")
    }
}

/// Generate a schema from the supported subset together with a conforming value.
fn generate(rng: &mut Rng, depth: usize) -> (Value, Value) {
    match rng.below(if depth == 0 { 4 } else { 6 }) {
        0 => (json!({ "type": "string" }), json!("kelp")),
        1 => {
            let choices = ["crab", "snail", "anemone"];
            let choice = choices[rng.below(3)];
            (json!({ "type": "string", "enum": choices }), json!(choice))
        }
        2 => {
            let minimum = i64::try_from(rng.below(5)).expect("small bound");
            let maximum = minimum + 1 + i64::try_from(rng.below(5)).expect("small bound");
            let span = u64::try_from(maximum - minimum + 1).expect("positive span");
            let value = minimum + i64::try_from(rng.below(span)).expect("small offset");
            (json!({ "type": "integer", "minimum": minimum, "maximum": maximum }), json!(value))
        }
        3 => (json!({ "type": "boolean" }), json!(rng.below(2) == 1)),
        4 => {
            let (items, element) = generate(rng, depth - 1);
            let count = 1 + rng.below(2);
            let values: Vec<Value> = (0..count).map(|_| element.clone()).collect();
            (
                json!({ "type": "array", "items": items, "minItems": 1, "maxItems": 3 }),
                Value::Array(values),
            )
        }
        _ => {
            let (first_schema, first_value) = generate(rng, depth - 1);
            let (second_schema, second_value) = generate(rng, depth - 1);
            let schema = json!({
                "type": "object",
                "properties": { "first": first_schema, "second": second_schema },
                "required": ["first"],
                "additionalProperties": false
            });
            let mut value = json!({ "first": first_value });
            if rng.below(2) == 1 {
                value["second"] = second_value;
            }
            (schema, value)
        }
    }
}

/// Break exactly one declared constraint of `value`.
fn violate(schema: &Value, value: &Value) -> Value {
    match schema["type"].as_str().expect("generated schemas declare a type") {
        "string" => json!(42),
        "integer" => {
            let maximum = schema["maximum"].as_i64().expect("generated bound");
            json!(maximum + 1)
        }
        "boolean" => json!("true"),
        "array" => {
            let mut items = value.as_array().cloned().expect("array value");
            items.push(json!({ "unexpected": true }));
            items.push(json!({ "unexpected": true }));
            items.push(json!({ "unexpected": true }));
            Value::Array(items)
        }
        _ => {
            let mut object = value.as_object().cloned().expect("object value");
            object.remove("first");
            Value::Object(object)
        }
    }
}

#[test]
fn generated_values_satisfy_their_schema_and_violations_are_rejected() {
    for seed in 0..200_u64 {
        let mut rng = Rng(seed.wrapping_mul(2_654_435_761).wrapping_add(12_345));
        let (source, value) = generate(&mut rng, 2);
        let schema = translate(source.clone())
            .unwrap_or_else(|error| panic!("generated schema {source} rejected: {error}"));

        schema.validate(&value).unwrap_or_else(|error| {
            panic!("conforming value {value} for {source} rejected: {error}")
        });

        let broken = violate(&source, &value);
        let error = schema
            .validate(&broken)
            .expect_err(&format!("violating value {broken} for {source} must be rejected"));
        assert_eq!(error.kind(), ErrorKind::Unknown);
    }
}

#[tokio::test]
async fn invalid_generated_json_fails_without_retrying() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::structured(["{\"animal\":7}"]));

    let error = session
        .prompt(
            vec![user("choose")],
            PromptOptions {
                response_constraint: Some(json!({
                    "type": "object",
                    "properties": { "animal": { "type": "string" } },
                    "required": ["animal"],
                    "additionalProperties": false
                })),
                omit_response_constraint_input: false,
            },
            Cancellation::new(),
        )
        .await
        .expect_err("output that violates the constraint fails");

    assert_eq!(error.kind(), ErrorKind::Unknown);
    assert_eq!(engine.requests().len(), 1, "a failed generation is not retried");
}

#[tokio::test]
async fn omitting_constraint_input_without_a_constraint_is_invalid_for_measurement() {
    let (engine, session) = scripted_session().await;
    let options = PromptOptions { response_constraint: None, omit_response_constraint_input: true };

    let error = session
        .measure_context_usage(vec![user("choose")], options.clone(), Cancellation::new())
        .await
        .expect_err("measuring with a suppressed, missing constraint is invalid");

    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert!(engine.requests().is_empty());
}

#[tokio::test]
async fn omitting_constraint_input_without_a_constraint_has_no_effect_on_generation() {
    let (engine, session) = scripted_session().await;
    engine.script(Script::text(["plain"]));

    let answer = session
        .prompt(
            vec![user("choose")],
            PromptOptions { response_constraint: None, omit_response_constraint_input: true },
            Cancellation::new(),
        )
        .await
        .expect("generation ignores a suppressed, missing constraint");

    assert_eq!(answer, "plain");
    assert!(engine.requests()[0].schema.is_none());
    assert!(!engine.requests()[0].include_schema_in_prompt);
}

#[tokio::test]
async fn unsupported_constraint_is_rejected_before_touching_the_session() {
    let (engine, session) = scripted_session().await;
    session.append(vec![user("history")], Cancellation::new()).await.expect("append");
    let usage = session.context_usage();
    let events = engine.log().len();

    let error = session
        .prompt(
            vec![user("choose")],
            PromptOptions {
                response_constraint: Some(json!({ "oneOf": [{ "type": "string" }] })),
                omit_response_constraint_input: false,
            },
            Cancellation::new(),
        )
        .await
        .expect_err("an unsupported constraint is rejected");

    assert_eq!(error.kind(), ErrorKind::NotSupported);
    assert_eq!(session.context_usage(), usage, "preflight failure changes nothing");
    assert_eq!(engine.log().len(), events);
    assert!(engine.requests().is_empty());
}

#[tokio::test]
async fn malformed_constraint_is_rejected_before_touching_the_session() {
    let (engine, session) = scripted_session().await;
    session.append(vec![user("history")], Cancellation::new()).await.expect("append");
    let usage = session.context_usage();
    let events = engine.log().len();

    let error = session
        .prompt(
            vec![user("choose")],
            PromptOptions {
                response_constraint: Some(json!({
                    "type": "object",
                    "properties": { "habitat": { "type": "string" } },
                    "required": ["habitat", "habitat"],
                    "additionalProperties": false
                })),
                omit_response_constraint_input: false,
            },
            Cancellation::new(),
        )
        .await
        .expect_err("a malformed constraint is rejected");

    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert_eq!(session.context_usage(), usage, "preflight failure changes nothing");
    assert_eq!(engine.log().len(), events);
    assert!(engine.requests().is_empty());
}
