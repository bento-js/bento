//! Lowering an accepted constraint into Apple's dynamic schema form.
//!
//! The framework resolves every named schema in one request through a single
//! namespace, so two nodes compiled under one name describe one type and one
//! node's constraints silently disappear. Identities therefore come from a
//! node's structural position, never from caller-supplied property names or
//! titles, which carry no uniqueness guarantee. A title annotates its node
//! instead of naming it.
//!
//! Only an object and a member property keep a description: the framework
//! inlines a string union as a plain enumerated string and gives arrays and
//! primitives no annotation of their own, so a name or description built onto
//! any of them disappears. An annotated node that sits where no description
//! survives — a root, or an array element — therefore compiles as a
//! one-alternative named union, which annotates the node while demanding
//! exactly the value the node demanded.

use foundation_models::{DynamicGenerationProperty, DynamicGenerationSchema, GenerationGuide};

use crate::schema::Node;

/// Lower an accepted schema into the dynamic form the framework compiles,
/// with its root compiled under `name`.
pub(super) fn dynamic_schema(root: &Node, name: &str) -> DynamicGenerationSchema {
    annotated(root, Identity::Named(name))
}

/// Where a node sits in the schema being lowered.
///
/// Objects and string unions are named types, and a wrapping union names the
/// node it annotates, so a slot renders a name solely for the nodes using one.
#[derive(Clone, Copy)]
enum Identity<'a> {
    /// The name the root compiles under.
    Named(&'a str),
    /// The slot at `position` under the node named `parent`.
    Slot { parent: &'a str, position: usize },
    /// The element of the array named `parent`.
    Element { parent: &'a str },
    /// The sole alternative of the union named `parent`.
    Wrapped { parent: &'a str },
}

impl Identity<'_> {
    /// The name this node compiles under.
    ///
    /// Positions are decimal and the structural segments are not, so no two
    /// of them collide; every parent prefixes its children, so a rendered
    /// name equals neither a sibling's, an ancestor's, nor the root name a
    /// caller's title supplied.
    fn name(self) -> String {
        match self {
            Self::Named(name) => name.to_owned(),
            Self::Slot { parent, position } => format!("{parent}_{position}"),
            Self::Element { parent } => format!("{parent}_item"),
            Self::Wrapped { parent } => format!("{parent}_value"),
        }
    }
}

/// Lower a node that nothing else can annotate: the root, or an array
/// element. A node that describes itself needs no help; any other annotated
/// node wears its annotation as a one-alternative union compiled under this
/// position's name, with the node itself compiled under the union's.
fn annotated(node: &Node, identity: Identity<'_>) -> DynamicGenerationSchema {
    if describes_itself(node) {
        return lower(node, identity);
    }
    let Some(annotation) = annotation(node, None) else {
        return lower(node, identity);
    };
    let name = identity.name();
    let alternative = lower(node, Identity::Wrapped { parent: &name });
    DynamicGenerationSchema::any_of(name, vec![alternative]).with_description(annotation)
}

fn lower(node: &Node, identity: Identity<'_>) -> DynamicGenerationSchema {
    match node {
        Node::Object { properties, .. } => {
            let name = identity.name();
            let mut schema = DynamicGenerationSchema::object(name.as_str());
            if let Some(annotation) = annotation(node, Some(name.as_str())) {
                schema = schema.with_description(annotation);
            }
            for (position, property) in properties.iter().enumerate() {
                let slot = Identity::Slot { parent: &name, position };
                let mut member = DynamicGenerationProperty::new(lower(&property.schema, slot))
                    .optional(!property.required);
                // An object describes itself; every other member reaches the
                // model only through the property that holds it.
                if !describes_itself(&property.schema)
                    && let Some(annotation) = annotation(&property.schema, None)
                {
                    member = member.with_description(annotation);
                }
                schema = schema.with_property(property.name.clone(), member);
            }
            schema
        }
        Node::Array { items, min_items, max_items, .. } => {
            let name = identity.name();
            let element = annotated(items, Identity::Element { parent: &name });
            DynamicGenerationSchema::array_of(element).with_element_bounds(*min_items, *max_items)
        }
        // A string union is named so that two unions never merge, but the
        // framework inlines it as an enumerated string and keeps neither the
        // name nor a description, so its annotation is placed like a
        // primitive's.
        Node::Text { choices: Some(choices), .. } => {
            DynamicGenerationSchema::any_of_strings(identity.name(), choices.clone())
        }
        Node::Text { choices: None, .. } => DynamicGenerationSchema::string(),
        Node::Integer { minimum, maximum, .. } => {
            DynamicGenerationSchema::integer().with_guides(guides(
                minimum.map(GenerationGuide::minimum_i64),
                maximum.map(GenerationGuide::maximum_i64),
            ))
        }
        Node::Number { minimum, maximum, .. } => {
            DynamicGenerationSchema::number().with_guides(guides(
                minimum.map(GenerationGuide::minimum_f64),
                maximum.map(GenerationGuide::maximum_f64),
            ))
        }
        Node::Boolean { .. } => DynamicGenerationSchema::boolean(),
    }
}

/// Whether a node compiles to a type that keeps its own description. Only an
/// object does.
fn describes_itself(node: &Node) -> bool {
    matches!(node, Node::Object { .. })
}

/// The description a node carries into the native schema: its own prose, with
/// the title folded in unless `name`, the name the node compiles under,
/// already carries it.
fn annotation(node: &Node, name: Option<&str>) -> Option<String> {
    let (title, description) = match node {
        Node::Object { title, description, .. }
        | Node::Array { title, description, .. }
        | Node::Text { title, description, .. }
        | Node::Integer { title, description, .. }
        | Node::Number { title, description, .. }
        | Node::Boolean { title, description } => (title.as_deref(), description.as_deref()),
    };
    match (title.filter(|title| Some(*title) != name), description) {
        (Some(title), Some(description)) => Some(format!("{title}: {description}")),
        (Some(annotation), None) | (None, Some(annotation)) => Some(annotation.to_owned()),
        (None, None) => None,
    }
}

fn guides(
    minimum: Option<GenerationGuide>,
    maximum: Option<GenerationGuide>,
) -> Vec<GenerationGuide> {
    minimum.into_iter().chain(maximum).collect()
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::engine::native::native_schema;
    use crate::schema::Schema;

    /// The framework's own view of `source`: the JSON schema the production
    /// path compiled the accepted constraint to, references and all.
    async fn compile(source: &Value) -> Value {
        let schema = Schema::translate(source).expect("schema translates");
        let compiled = native_schema(&schema).await.expect("schema compiles");
        serde_json::from_str(compiled.json_schema()).expect("compiled schema is JSON")
    }

    /// The compiled subschema at `path`, with references followed, so
    /// assertions read the constraints the framework applies rather than the
    /// names it filed them under.
    fn at<'a>(document: &'a Value, path: &[&str]) -> &'a Value {
        resolve(document, declared(document, path))
    }

    /// The subschema `path` declares, before the reference it may name is
    /// followed.
    fn declared<'a>(document: &'a Value, path: &[&str]) -> &'a Value {
        let (name, parents) = path.split_last().expect("a path names a property");
        let mut node = resolve(document, document);
        for &parent in parents {
            let child = property(node, parent)
                .unwrap_or_else(|| panic!("{parent} is missing from {node} in {document}"));
            node = resolve(document, child);
        }
        property(node, name)
            .unwrap_or_else(|| panic!("{name} is missing from {node} in {document}"))
    }

    /// The descriptions the compiled schema files for the node at `path`: the
    /// framework may keep an annotation where the property is declared, in the
    /// definition it resolves to, or both.
    fn annotations<'a>(document: &'a Value, path: &[&str]) -> Vec<&'a str> {
        filed(document, declared(document, path))
    }

    /// The descriptions the compiled schema files for one node, wherever the
    /// framework kept them.
    fn filed<'a>(document: &'a Value, node: &'a Value) -> Vec<&'a str> {
        [node, resolve(document, node)]
            .into_iter()
            .filter_map(|node| node.get("description").and_then(Value::as_str))
            .collect()
    }

    /// The element schema a compiled array declares, before the reference it
    /// may name is followed.
    fn element<'a>(document: &'a Value, path: &[&str]) -> &'a Value {
        let array = at(document, path);
        array
            .get("items")
            .unwrap_or_else(|| panic!("{array} declares no element schema in {document}"))
    }

    /// Follow a `$ref` to the definition it names.
    fn resolve<'a>(document: &'a Value, node: &'a Value) -> &'a Value {
        let Some(reference) = node.get("$ref").and_then(Value::as_str) else {
            return node;
        };
        let pointer = reference.trim_start_matches('#').trim_start_matches('/');
        if pointer.is_empty() {
            return document;
        }
        let mut target = document;
        for segment in pointer.split('/') {
            target = target
                .get(segment)
                .unwrap_or_else(|| panic!("{reference} is unresolved in {document}"));
        }
        target
    }

    fn property<'a>(node: &'a Value, name: &str) -> Option<&'a Value> {
        node.get("properties").and_then(|properties| properties.get(name))
    }

    /// The property names a compiled object demands.
    fn required(node: &Value) -> Vec<&str> {
        node.get("required")
            .and_then(Value::as_array)
            .map(|names| names.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    }

    #[tokio::test]
    async fn repeated_nested_names_keep_independent_constraints() {
        let document = compile(&json!({
            "type": "object",
            "properties": {
                "left": {
                    "type": "object",
                    "properties": {
                        "entry": {
                            "type": "object",
                            "properties": { "x": { "type": "string" } },
                            "required": ["x"],
                        },
                    },
                    "required": ["entry"],
                },
                "right": {
                    "type": "object",
                    "properties": {
                        "entry": {
                            "type": "object",
                            "properties": { "y": { "type": "integer" } },
                            "required": ["y"],
                        },
                    },
                    "required": ["entry"],
                },
            },
            "required": ["left", "right"],
        }))
        .await;

        let left = at(&document, &["left", "entry"]);
        let right = at(&document, &["right", "entry"]);
        assert!(required(left).contains(&"x"), "left entry lost its requirement: {left}");
        assert!(required(right).contains(&"y"), "right entry lost its requirement: {right}");
        assert!(property(left, "y").is_none(), "left entry took the right's shape: {left}");
        assert!(property(right, "x").is_none(), "right entry took the left's shape: {right}");
        assert_eq!(at(&document, &["left", "entry", "x"])["type"], json!("string"));
        assert_eq!(at(&document, &["right", "entry", "y"])["type"], json!("integer"));
    }

    #[tokio::test]
    async fn repeated_enum_names_keep_their_own_choices() {
        let document = compile(&json!({
            "type": "object",
            "properties": {
                "left": {
                    "type": "object",
                    "properties": { "choice": { "type": "string", "enum": ["ruby", "amber"] } },
                    "required": ["choice"],
                },
                "right": {
                    "type": "object",
                    "properties": { "choice": { "type": "string", "enum": ["jade", "slate"] } },
                    "required": ["choice"],
                },
            },
            "required": ["left", "right"],
        }))
        .await;

        let left = at(&document, &["left", "choice"]).to_string();
        let right = at(&document, &["right", "choice"]).to_string();
        assert!(
            left.contains("ruby") && left.contains("amber"),
            "left choice lost its alternatives: {left}"
        );
        assert!(
            !left.contains("jade") && !left.contains("slate"),
            "left choice took the right's alternatives: {left}"
        );
        assert!(
            right.contains("jade") && right.contains("slate"),
            "right choice lost its alternatives: {right}"
        );
        assert!(
            !right.contains("ruby") && !right.contains("amber"),
            "right choice took the left's alternatives: {right}"
        );
    }

    #[tokio::test]
    async fn repeated_titles_annotate_without_merging_shapes() {
        let document = compile(&json!({
            "type": "object",
            "properties": {
                "first": {
                    "type": "object",
                    "title": "Entry",
                    "description": "the opening entry",
                    "properties": { "x": { "type": "string" } },
                    "required": ["x"],
                },
                "second": {
                    "type": "object",
                    "title": "Entry",
                    "properties": { "y": { "type": "integer" } },
                    "required": ["y"],
                },
            },
            "required": ["first", "second"],
        }))
        .await;

        let first = at(&document, &["first"]);
        let second = at(&document, &["second"]);
        assert!(required(first).contains(&"x"), "first entry lost its requirement: {first}");
        assert!(required(second).contains(&"y"), "second entry lost its requirement: {second}");
        assert!(property(first, "y").is_none(), "first entry took the second's shape: {first}");
        assert!(property(second, "x").is_none(), "second entry took the first's shape: {second}");

        let first_annotations = annotations(&document, &["first"]);
        let second_annotations = annotations(&document, &["second"]);
        assert!(
            first_annotations.iter().any(|annotation| annotation.contains("Entry")),
            "the first entry lost the title that annotates it: {document}"
        );
        assert!(
            first_annotations.iter().any(|annotation| annotation.contains("the opening entry")),
            "the first entry lost its description: {document}"
        );
        assert!(
            second_annotations.iter().any(|annotation| annotation.contains("Entry")),
            "the second entry lost the title that annotates it: {document}"
        );
    }

    #[tokio::test]
    async fn scalar_properties_keep_their_own_annotations() {
        let document = compile(&json!({
            "type": "object",
            "properties": {
                "habitat": { "type": "string", "title": "Habitat" },
                "depth": {
                    "type": "number",
                    "title": "Depth",
                    "description": "metres below the surface",
                },
                "legs": { "type": "integer", "title": "Legs" },
                "shelled": { "type": "boolean", "title": "Shelled" },
            },
            "required": ["habitat", "depth", "legs", "shelled"],
        }))
        .await;

        let labelled =
            [("habitat", "Habitat"), ("depth", "Depth"), ("legs", "Legs"), ("shelled", "Shelled")];
        for (name, label) in labelled {
            let kept = annotations(&document, &[name]);
            assert!(
                kept.iter().any(|annotation| annotation.contains(label)),
                "{name} lost the title that annotates it: {document}"
            );
            for (other, stranger) in labelled {
                assert!(
                    other == name || !kept.iter().any(|annotation| annotation.contains(stranger)),
                    "{name} took {other}'s annotation: {document}"
                );
            }
        }

        assert!(
            annotations(&document, &["depth"])
                .iter()
                .any(|annotation| annotation.contains("metres below the surface")),
            "the number lost its description: {document}"
        );
    }

    #[tokio::test]
    async fn an_annotated_array_property_keeps_its_own_annotation() {
        let document = compile(&json!({
            "type": "object",
            "properties": {
                "tides": {
                    "type": "array",
                    "title": "Tides",
                    "description": "one entry per observed tide",
                    "items": { "type": "string" },
                    "minItems": 1,
                },
                "notes": { "type": "array", "items": { "type": "string" } },
            },
            "required": ["tides", "notes"],
        }))
        .await;

        let tides = annotations(&document, &["tides"]);
        assert!(
            tides.iter().any(|annotation| annotation.contains("Tides")),
            "the array lost the title that annotates it: {document}"
        );
        assert!(
            tides.iter().any(|annotation| annotation.contains("one entry per observed tide")),
            "the array lost its description: {document}"
        );
        assert!(
            !annotations(&document, &["notes"])
                .iter()
                .any(|annotation| annotation.contains("Tides")),
            "the unannotated array took its sibling's annotation: {document}"
        );
    }

    #[tokio::test]
    async fn nested_items_keep_their_own_annotations() {
        let document = compile(&json!({
            "type": "object",
            "properties": {
                "shore": {
                    "type": "array",
                    "title": "Shoreline",
                    "items": {
                        "type": "object",
                        "title": "Pool",
                        "description": "one tide pool",
                        "properties": { "animal": { "type": "string", "title": "Animal" } },
                        "required": ["animal"],
                    },
                },
                "moods": {
                    "type": "array",
                    "items": {
                        "type": "string",
                        "title": "Mood",
                        "description": "how the water moved",
                        "enum": ["calm", "choppy"],
                    },
                },
            },
            "required": ["shore", "moods"],
        }))
        .await;

        assert!(
            annotations(&document, &["shore"])
                .iter()
                .any(|annotation| annotation.contains("Shoreline")),
            "the array of pools lost the title that annotates it: {document}"
        );

        let pool = filed(&document, element(&document, &["shore"]));
        assert!(
            pool.iter().any(|annotation| annotation.contains("Pool")),
            "the item object lost the title that annotates it: {document}"
        );
        assert!(
            pool.iter().any(|annotation| annotation.contains("one tide pool")),
            "the item object lost its description: {document}"
        );

        let item = resolve(&document, element(&document, &["shore"]));
        let animal = property(item, "animal")
            .unwrap_or_else(|| panic!("the item object lost its property: {document}"));
        assert!(
            filed(&document, animal).iter().any(|annotation| annotation.contains("Animal")),
            "a property inside an array item lost the title that annotates it: {document}"
        );

        let mood = filed(&document, element(&document, &["moods"]));
        assert!(
            mood.iter().any(|annotation| annotation.contains("Mood")),
            "the enum item lost the title that annotates it: {document}"
        );
        assert!(
            mood.iter().any(|annotation| annotation.contains("how the water moved")),
            "the enum item lost its description: {document}"
        );
    }

    #[tokio::test]
    async fn a_non_object_root_keeps_its_annotation() {
        let scalar = compile(&json!({
            "type": "string",
            "title": "Colour",
            "description": "the colour of the water",
        }))
        .await;
        let kept = filed(&scalar, &scalar);
        assert!(
            kept.iter().any(|annotation| annotation.contains("Colour")),
            "the root string lost the title that annotates it: {scalar}"
        );
        assert!(
            kept.iter().any(|annotation| annotation.contains("the colour of the water")),
            "the root string lost its description: {scalar}"
        );

        let listing = compile(&json!({
            "type": "array",
            "title": "Catches",
            "description": "everything the net held",
            "items": { "type": "string" },
        }))
        .await;
        let kept = filed(&listing, &listing);
        assert!(
            kept.iter().any(|annotation| annotation.contains("Catches")),
            "the root array lost the title that annotates it: {listing}"
        );
        assert!(
            kept.iter().any(|annotation| annotation.contains("everything the net held")),
            "the root array lost its description: {listing}"
        );
    }

    #[tokio::test]
    async fn array_items_keep_their_own_annotations() {
        let document = compile(&json!({
            "type": "object",
            "properties": {
                "depths": {
                    "type": "array",
                    "items": {
                        "type": "number",
                        "title": "Depth",
                        "description": "metres below the surface",
                    },
                },
                "rows": {
                    "type": "array",
                    "items": {
                        "type": "array",
                        "title": "Row",
                        "description": "one row of readings",
                        "items": { "type": "integer" },
                    },
                },
            },
            "required": ["depths", "rows"],
        }))
        .await;

        let depth = filed(&document, element(&document, &["depths"]));
        assert!(
            depth.iter().any(|annotation| annotation.contains("Depth")),
            "the number item lost the title that annotates it: {document}"
        );
        assert!(
            depth.iter().any(|annotation| annotation.contains("metres below the surface")),
            "the number item lost its description: {document}"
        );

        let row = filed(&document, element(&document, &["rows"]));
        assert!(
            row.iter().any(|annotation| annotation.contains("Row")),
            "the array item lost the title that annotates it: {document}"
        );
        assert!(
            row.iter().any(|annotation| annotation.contains("one row of readings")),
            "the array item lost its description: {document}"
        );
    }

    #[tokio::test]
    async fn enum_choices_keep_their_own_annotations_at_every_placement() {
        let root = compile(&json!({
            "type": "string",
            "enum": ["calm", "choppy"],
            "title": "Mood",
            "description": "how the water moved",
        }))
        .await;
        let kept = filed(&root, &root);
        assert!(
            kept.iter().any(|annotation| annotation.contains("Mood")),
            "the root enum lost the title that annotates it: {root}"
        );
        assert!(
            kept.iter().any(|annotation| annotation.contains("how the water moved")),
            "the root enum lost its description: {root}"
        );
        let demanded = root.to_string();
        assert!(
            demanded.contains("calm") && demanded.contains("choppy"),
            "annotating the root enum changed the value it demands: {root}"
        );

        let document = compile(&json!({
            "type": "object",
            "properties": {
                "mood": {
                    "type": "string",
                    "enum": ["calm", "choppy"],
                    "title": "Mood",
                    "description": "how the water moved",
                },
                "tone": { "type": "string", "enum": ["warm", "cold"] },
            },
            "required": ["mood", "tone"],
        }))
        .await;
        let mood = annotations(&document, &["mood"]);
        assert!(
            mood.iter().any(|annotation| annotation.contains("Mood")),
            "the enum member lost the title that annotates it: {document}"
        );
        assert!(
            mood.iter().any(|annotation| annotation.contains("how the water moved")),
            "the enum member lost its description: {document}"
        );
        assert!(
            !annotations(&document, &["tone"]).iter().any(|annotation| annotation.contains("Mood")),
            "the unannotated enum member took its sibling's annotation: {document}"
        );
    }

    #[tokio::test]
    async fn a_root_title_matching_the_compiled_name_still_annotates() {
        let schema = Schema::translate(&json!({ "type": "string" })).expect("schema translates");
        let title = schema.name();
        let scalar = compile(&json!({ "type": "string", "title": title })).await;
        assert!(
            scalar.get("title").and_then(Value::as_str) == Some(title.as_str())
                || filed(&scalar, &scalar).iter().any(|annotation| annotation.contains(&title)),
            "the root string's only annotation vanished into its compiled name: {scalar}"
        );
    }
}
