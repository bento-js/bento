//! The closed JSON Schema subset accepted for constrained generation, its
//! translation into typed nodes, and validation of generated output.
//!
//! Supported keywords are `type` (`object`, `array`, `string`, `integer`,
//! `number`, `boolean`), `properties`, `required`, `additionalProperties`
//! (only `false`), `items`, `minItems`, `maxItems`, `minimum`, `maximum`,
//! string-valued `enum`, `title` and `description`. Anything else is reported
//! rather than discarded.
//!
//! A keyword that does not apply to the declared type, and an integer bound or
//! item count outside the signed 64-bit range the model's generation guides
//! carry, are reported as unsupported rather than quietly ignored. Any JSON
//! number with a zero fractional part is an integer.

use std::cmp::Ordering;

use serde_json::{Map, Value};

use crate::{Error, ErrorKind};

const SUPPORTED_KEYWORDS: [&str; 12] = [
    "type",
    "title",
    "description",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "minItems",
    "maxItems",
    "enum",
    "minimum",
    "maximum",
];

/// One translated schema node. Every node carries the annotations it was
/// given; where the model's schema form has no slot for them, lowering finds
/// one rather than dropping them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Node {
    Object {
        title: Option<String>,
        description: Option<String>,
        properties: Vec<Property>,
    },
    Array {
        title: Option<String>,
        description: Option<String>,
        items: Box<Node>,
        min_items: Option<usize>,
        max_items: Option<usize>,
    },
    Text {
        title: Option<String>,
        description: Option<String>,
        choices: Option<Vec<String>>,
    },
    Integer {
        title: Option<String>,
        description: Option<String>,
        minimum: Option<i64>,
        maximum: Option<i64>,
    },
    Number {
        title: Option<String>,
        description: Option<String>,
        minimum: Option<f64>,
        maximum: Option<f64>,
    },
    Boolean {
        title: Option<String>,
        description: Option<String>,
    },
}

/// One translated object property.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Property {
    pub(crate) name: String,
    pub(crate) schema: Node,
    pub(crate) required: bool,
}

/// An accepted constraint: the schema as supplied, plus its typed translation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Schema {
    source: Value,
    root: Node,
}

impl Schema {
    /// Validate and translate a caller-supplied JSON Schema.
    pub(crate) fn translate(source: &Value) -> Result<Self, Error> {
        let root = translate_node(source)?;
        Ok(Self { source: source.clone(), root })
    }

    /// The schema exactly as supplied, for accounting and native requests.
    pub(crate) fn source(&self) -> &Value {
        &self.source
    }

    /// The typed translation a compiled engine turns into a native schema.
    #[cfg(native_backend)]
    pub(crate) fn root(&self) -> &Node {
        &self.root
    }

    /// Name the model generates under: the root schema's title, or a default.
    pub(crate) fn name(&self) -> String {
        match &self.root {
            Node::Object { title: Some(title), .. } => title.clone(),
            _ => "Response".to_owned(),
        }
    }

    /// Check generated output against the accepted constraint.
    pub(crate) fn validate(&self, value: &Value) -> Result<(), Error> {
        validate_node(&self.root, value, "value")
    }

    /// Parse and check generated JSON text.
    pub(crate) fn validate_text(&self, text: &str) -> Result<(), Error> {
        let value: Value = serde_json::from_str(text).map_err(|error| {
            Error::new(ErrorKind::Unknown, format!("the model returned invalid JSON: {error}"))
        })?;
        self.validate(&value)
    }
}

fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidInput, message)
}

fn unsupported(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::NotSupported, message)
}

fn mismatch(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::Unknown, message)
}

fn translate_node(value: &Value) -> Result<Node, Error> {
    let object = value.as_object().ok_or_else(|| invalid("a schema must be a JSON object"))?;

    for keyword in object.keys() {
        if !SUPPORTED_KEYWORDS.contains(&keyword.as_str()) {
            return Err(unsupported(format!(
                "`{keyword}` is outside the supported JSON Schema subset"
            )));
        }
    }

    let declared =
        object.get("type").ok_or_else(|| invalid("a schema must declare a supported `type`"))?;
    let kind = match declared {
        Value::String(kind) => kind.as_str(),
        Value::Array(_) => return Err(unsupported("union types are not supported")),
        _ => return Err(invalid("`type` must be a string")),
    };

    let title = optional_string(object, "title")?;
    let description = optional_string(object, "description")?;

    match kind {
        "object" => {
            deny_inapplicable(object, kind, &["properties", "required", "additionalProperties"])?;
            Ok(Node::Object { title, description, properties: translate_properties(object)? })
        }
        "array" => {
            deny_inapplicable(object, kind, &["items", "minItems", "maxItems"])?;
            let items = match object.get("items") {
                Some(Value::Array(_)) => {
                    return Err(unsupported(
                        "tuple `items` are not supported; arrays are homogeneous",
                    ));
                }
                Some(items) => translate_node(items)?,
                None => return Err(invalid("an array schema must declare `items`")),
            };
            let min_items = optional_count(object, "minItems")?;
            let max_items = optional_count(object, "maxItems")?;
            if let (Some(minimum), Some(maximum)) = (min_items, max_items)
                && minimum > maximum
            {
                return Err(invalid("`minItems` must not exceed `maxItems`"));
            }
            Ok(Node::Array { title, description, items: Box::new(items), min_items, max_items })
        }
        "string" => {
            deny_inapplicable(object, kind, &["enum"])?;
            Ok(Node::Text { title, description, choices: translate_choices(object)? })
        }
        "integer" => {
            deny_inapplicable(object, kind, &["minimum", "maximum"])?;
            let minimum = optional_integer_bound(object, "minimum", f64::ceil)?;
            let maximum = optional_integer_bound(object, "maximum", f64::floor)?;
            if let (Some(low), Some(high)) = (minimum, maximum)
                && low > high
            {
                return Err(invalid("`minimum` and `maximum` leave no permitted integer"));
            }
            Ok(Node::Integer { title, description, minimum, maximum })
        }
        "number" => {
            deny_inapplicable(object, kind, &["minimum", "maximum"])?;
            let minimum = optional_number(object, "minimum")?;
            let maximum = optional_number(object, "maximum")?;
            if let (Some(low), Some(high)) = (minimum, maximum)
                && low > high
            {
                return Err(invalid("`minimum` must not exceed `maximum`"));
            }
            Ok(Node::Number { title, description, minimum, maximum })
        }
        "boolean" => {
            deny_inapplicable(object, kind, &[])?;
            Ok(Node::Boolean { title, description })
        }
        "null" => Err(unsupported("`null` schemas are not supported")),
        other => Err(unsupported(format!("`{other}` is not a supported schema type"))),
    }
}

fn deny_inapplicable(
    object: &Map<String, Value>,
    kind: &str,
    applicable: &[&str],
) -> Result<(), Error> {
    for keyword in object.keys() {
        let common = matches!(keyword.as_str(), "type" | "title" | "description");
        if !common && !applicable.contains(&keyword.as_str()) {
            return Err(unsupported(format!("`{keyword}` is not supported on a {kind} schema")));
        }
    }
    Ok(())
}

fn translate_properties(object: &Map<String, Value>) -> Result<Vec<Property>, Error> {
    match object.get("additionalProperties") {
        None | Some(Value::Bool(false)) => {}
        Some(Value::Bool(true)) => {
            return Err(unsupported("`additionalProperties: true` is not supported"));
        }
        Some(Value::Object(_)) => {
            return Err(unsupported("an `additionalProperties` schema is not supported"));
        }
        Some(_) => return Err(invalid("`additionalProperties` must be `false`")),
    }

    let declared = match object.get("properties") {
        None => None,
        Some(Value::Object(properties)) => Some(properties),
        Some(_) => return Err(invalid("`properties` must be an object")),
    };

    let required = match object.get("required") {
        None => Vec::new(),
        Some(Value::Array(names)) => names
            .iter()
            .map(|name| name.as_str().ok_or_else(|| invalid("`required` must list property names")))
            .collect::<Result<Vec<&str>, Error>>()?,
        Some(_) => return Err(invalid("`required` must be an array of property names")),
    };

    for (index, name) in required.iter().enumerate() {
        if required[..index].contains(name) {
            return Err(invalid(format!("`required` names `{name}` more than once")));
        }
        if !declared.is_some_and(|properties| properties.contains_key(*name)) {
            return Err(invalid(format!("`required` names undeclared property `{name}`")));
        }
    }

    declared
        .into_iter()
        .flatten()
        .map(|(name, schema)| {
            Ok(Property {
                name: name.clone(),
                schema: translate_node(schema)?,
                required: required.contains(&name.as_str()),
            })
        })
        .collect()
}

fn translate_choices(object: &Map<String, Value>) -> Result<Option<Vec<String>>, Error> {
    let Some(choices) = object.get("enum") else {
        return Ok(None);
    };
    let Value::Array(choices) = choices else {
        return Err(invalid("`enum` must be an array of strings"));
    };
    if choices.is_empty() {
        return Err(invalid("`enum` must list at least one value"));
    }
    choices
        .iter()
        .map(|choice| {
            choice
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| unsupported("only string-valued `enum` constraints are supported"))
        })
        .collect::<Result<Vec<String>, Error>>()
        .map(Some)
}

fn optional_string(object: &Map<String, Value>, keyword: &str) -> Result<Option<String>, Error> {
    match object.get(keyword) {
        None => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(invalid(format!("`{keyword}` must be a string"))),
    }
}

/// A non-negative item count. JSON Schema item counts are integers, and a JSON
/// number with a zero fractional part is an integer; a count beyond the range
/// the model's element bounds carry is reported rather than silently dropped.
fn optional_count(object: &Map<String, Value>, keyword: &str) -> Result<Option<usize>, Error> {
    let Some(value) = object.get(keyword) else {
        return Ok(None);
    };
    let malformed = || invalid(format!("`{keyword}` must be a non-negative integer"));
    let count = match value.as_i64() {
        Some(count) => count,
        None => {
            let number = value
                .as_f64()
                .filter(|number| *number >= 0.0 && number.fract() == 0.0)
                .ok_or_else(malformed)?;
            integral(number).ok_or_else(|| {
                unsupported(format!("`{keyword}` is outside the supported 64-bit count range"))
            })?
        }
    };
    usize::try_from(count).map(Some).map_err(|_| malformed())
}

fn optional_number(object: &Map<String, Value>, keyword: &str) -> Result<Option<f64>, Error> {
    let Some(value) = object.get(keyword) else {
        return Ok(None);
    };
    let number = value.as_f64().ok_or_else(|| invalid(format!("`{keyword}` must be a number")))?;
    if compare_number(value, number, number) != Ordering::Equal {
        return Err(unsupported(format!(
            "`{keyword}` cannot be represented exactly by the native number format"
        )));
    }
    Ok(Some(number))
}

/// A numeric bound of an integer schema. Fractional bounds are permitted by
/// JSON Schema, so they are narrowed to the nearest integer the model may
/// produce rather than rejected. A bound beyond the range the model's guides
/// carry is reported rather than narrowed into a different constraint.
fn optional_integer_bound(
    object: &Map<String, Value>,
    keyword: &str,
    narrow: fn(f64) -> f64,
) -> Result<Option<i64>, Error> {
    let Some(value) = object.get(keyword) else {
        return Ok(None);
    };
    if let Some(bound) = value.as_i64() {
        return Ok(Some(bound));
    }
    let number = value
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| invalid(format!("`{keyword}` must be a number")))?;
    integral(narrow(number)).map(Some).ok_or_else(|| {
        unsupported(format!("`{keyword}` is outside the supported 64-bit integer range"))
    })
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "the value is integral and checked against the i64 range first"
)]
fn integral(value: f64) -> Option<i64> {
    const LIMIT: f64 = 9_223_372_036_854_775_808.0;
    if (-LIMIT..LIMIT).contains(&value) { Some(value as i64) } else { None }
}

/// Generated output as an exact integer. Any JSON number with a zero
/// fractional part is an integer, whatever its spelling.
fn integer_output(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|number| i64::try_from(number).ok()))
        .or_else(|| value.as_f64().filter(|number| number.fract() == 0.0).and_then(integral))
}

fn validate_node(node: &Node, value: &Value, path: &str) -> Result<(), Error> {
    match node {
        Node::Object { properties, .. } => {
            let object =
                value.as_object().ok_or_else(|| mismatch(format!("{path} is not an object")))?;
            for property in properties {
                match object.get(&property.name) {
                    Some(member) => validate_node(
                        &property.schema,
                        member,
                        &format!("{path}.{}", property.name),
                    )?,
                    None if property.required => {
                        return Err(mismatch(format!(
                            "{path} is missing required property `{}`",
                            property.name
                        )));
                    }
                    None => {}
                }
            }
            for name in object.keys() {
                if !properties.iter().any(|property| &property.name == name) {
                    return Err(mismatch(format!("{path} has undeclared property `{name}`")));
                }
            }
            Ok(())
        }
        Node::Array { items, min_items, max_items, .. } => {
            let array =
                value.as_array().ok_or_else(|| mismatch(format!("{path} is not an array")))?;
            if min_items.is_some_and(|minimum| array.len() < minimum) {
                return Err(mismatch(format!("{path} has fewer items than `minItems`")));
            }
            if max_items.is_some_and(|maximum| array.len() > maximum) {
                return Err(mismatch(format!("{path} has more items than `maxItems`")));
            }
            for (index, item) in array.iter().enumerate() {
                validate_node(items, item, &format!("{path}[{index}]"))?;
            }
            Ok(())
        }
        Node::Text { choices, .. } => {
            let text = value.as_str().ok_or_else(|| mismatch(format!("{path} is not a string")))?;
            match choices {
                Some(choices) if !choices.iter().any(|choice| choice == text) => {
                    Err(mismatch(format!("{path} is not one of the enumerated values")))
                }
                _ => Ok(()),
            }
        }
        Node::Integer { minimum, maximum, .. } => {
            let number = integer_output(value)
                .ok_or_else(|| mismatch(format!("{path} is not an integer")))?;
            if minimum.is_some_and(|bound| number < bound) {
                return Err(mismatch(format!("{path} is below `minimum`")));
            }
            if maximum.is_some_and(|bound| number > bound) {
                return Err(mismatch(format!("{path} is above `maximum`")));
            }
            Ok(())
        }
        Node::Number { minimum, maximum, .. } => {
            let number =
                value.as_f64().ok_or_else(|| mismatch(format!("{path} is not a number")))?;
            check_bounds(value, number, *minimum, *maximum, path)
        }
        Node::Boolean { .. } => {
            value.as_bool().map(|_| ()).ok_or_else(|| mismatch(format!("{path} is not a boolean")))
        }
    }
}

fn check_bounds(
    value: &Value,
    number: f64,
    minimum: Option<f64>,
    maximum: Option<f64>,
    path: &str,
) -> Result<(), Error> {
    if minimum.is_some_and(|bound| compare_number(value, number, bound) == Ordering::Less) {
        return Err(mismatch(format!("{path} is below `minimum`")));
    }
    if maximum.is_some_and(|bound| compare_number(value, number, bound) == Ordering::Greater) {
        return Err(mismatch(format!("{path} is above `maximum`")));
    }
    Ok(())
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "a bound equal to an i64/u64 float approximation is integral and fits i128"
)]
fn compare_number(value: &Value, approximation: f64, bound: f64) -> Ordering {
    if approximation < bound {
        return Ordering::Less;
    }
    if approximation > bound {
        return Ordering::Greater;
    }
    let integer = value.as_i64().map(i128::from).or_else(|| value.as_u64().map(i128::from));
    integer.map_or(Ordering::Equal, |integer| integer.cmp(&(bound as i128)))
}
