mod common;

use bento_compiler::CompilerSettings;
use common::transformed;

const SOURCE: &str = r#"import { log } from "./log";
interface Options { loud: boolean }

export function greet(name: string, options: Options): void {
    log(options.loud ? name.toUpperCase() : name);
}
"#;

/// One decoded `mappings` segment: generated and original positions are zero-based.
#[derive(Debug, PartialEq, Eq)]
struct Mapping {
    generated_line: usize,
    generated_column: usize,
    source_index: usize,
    original_line: usize,
    original_column: usize,
}

fn decode_vlq(segment: &str) -> Vec<i64> {
    const ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut fields = Vec::new();
    let mut accumulator: i64 = 0;
    let mut shift = 0;
    for character in segment.chars() {
        let digit = ALPHABET.find(character).expect("base64 VLQ digit") as i64;
        accumulator |= (digit & 0b11111) << shift;
        shift += 5;
        if digit & 0b100000 == 0 {
            let negative = accumulator & 1 == 1;
            let magnitude = accumulator >> 1;
            fields.push(if negative { -magnitude } else { magnitude });
            accumulator = 0;
            shift = 0;
        }
    }
    fields
}

fn decode_mappings(mappings: &str) -> Vec<Mapping> {
    let mut decoded = Vec::new();
    let (mut source_index, mut original_line, mut original_column) = (0i64, 0i64, 0i64);
    for (generated_line, line) in mappings.split(';').enumerate() {
        let mut generated_column = 0i64;
        for segment in line.split(',').filter(|segment| !segment.is_empty()) {
            let fields = decode_vlq(segment);
            generated_column += fields[0];
            if fields.len() >= 4 {
                source_index += fields[1];
                original_line += fields[2];
                original_column += fields[3];
                decoded.push(Mapping {
                    generated_line,
                    generated_column: usize::try_from(generated_column).unwrap(),
                    source_index: usize::try_from(source_index).unwrap(),
                    original_line: usize::try_from(original_line).unwrap(),
                    original_column: usize::try_from(original_column).unwrap(),
                });
            }
        }
    }
    decoded
}

fn position_of(text: &str, needle: &str) -> (usize, usize) {
    let offset = text.find(needle).unwrap_or_else(|| panic!("{needle:?} not in {text:?}"));
    let line = text[..offset].matches('\n').count();
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    (line, offset - line_start)
}

#[test]
fn source_map_is_a_v3_map_that_sends_generated_positions_back_to_the_source() {
    let output = transformed("/project/src/greet.ts", SOURCE, &CompilerSettings::default());
    let map: serde_json::Value = serde_json::from_str(&output.source_map).unwrap();

    assert_eq!(map["version"], 3);
    assert_eq!(map["sources"], serde_json::json!(["/project/src/greet.ts"]));
    assert_eq!(map["sourcesContent"], serde_json::json!([SOURCE]));

    let mappings = decode_mappings(map["mappings"].as_str().unwrap());
    let (generated_line, generated_column) = position_of(&output.code, "toUpperCase");
    let (original_line, original_column) = position_of(SOURCE, "toUpperCase");
    assert_ne!(generated_line, original_line, "the interface line must have been removed");

    let mapping = mappings
        .iter()
        .find(|mapping| {
            mapping.generated_line == generated_line && mapping.generated_column == generated_column
        })
        .unwrap_or_else(|| {
            panic!("no mapping at {generated_line}:{generated_column}\n{mappings:#?}")
        });
    assert_eq!(mapping.source_index, 0);
    assert_eq!((mapping.original_line, mapping.original_column), (original_line, original_column));
}
