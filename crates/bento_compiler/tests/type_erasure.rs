mod common;

use bento_compiler::CompilerSettings;
use common::code;

const TYPED_SOURCE: &str = r#"
import type { Shape } from "./shape";
import { area } from "./area";
export type { Shape };

interface Point { x: number; y: number }

export enum Color { Red, Green = "green" }

export class Circle<T extends Point> {
    declare tag: string;
    constructor(private readonly center: T, public radius = 1) {}
}

const origin = { x: 0, y: 0 } satisfies Point;
const label = origin as unknown as string;
export const total: number = area<Point>(origin, label);
"#;

#[test]
fn type_syntax_is_removed_and_runtime_constructs_remain() {
    let output = code("/project/geometry.ts", TYPED_SOURCE, &CompilerSettings::default());

    assert!(!output.contains("interface"), "{output}");
    assert!(!output.contains("satisfies"), "{output}");
    assert!(!output.contains(" as "), "{output}");
    assert!(!output.contains(": number"), "{output}");
    assert!(!output.contains("<Point>"), "{output}");
    assert!(!output.contains("declare"), "{output}");
    assert!(!output.contains("tag"), "{output}");
    assert!(!output.contains("Shape"), "{output}");
    assert!(!output.contains("export type"), "{output}");

    assert!(output.contains(r#"import { area } from "./area""#), "{output}");
    assert!(output.contains(r#"Color[Color["Red"] = 0] = "Red""#), "{output}");
    assert!(output.contains(r#"Color["Green"] = "green""#), "{output}");
    assert!(output.contains("this.center = center"), "{output}");
    assert!(output.contains("this.radius = radius"), "{output}");
    assert!(output.contains("area(origin, label)"), "{output}");
}

#[test]
fn a_type_error_is_not_checked_and_its_annotation_is_removed() {
    let source = "const n: number = \"x\";\nexport const ok: boolean = n.length > 0;\n";
    let output = code("/project/mistyped.ts", source, &CompilerSettings::default());

    assert!(output.contains(r#"const n = "x";"#), "{output}");
    assert!(output.contains("export const ok = n.length > 0;"), "{output}");
    assert!(!output.contains("number"), "{output}");
}

const IMPORT_SOURCE: &str = r#"
import type { Shape } from "./shape";
import { Big } from "./big";
import { unused } from "./side-effects";
import { used } from "./used";
let big: Big;
export const value = used();
"#;

#[test]
fn verbatim_module_syntax_removes_only_type_imports() {
    let settings = CompilerSettings { verbatim_module_syntax: true, ..CompilerSettings::default() };
    let output = code("/project/imports.ts", IMPORT_SOURCE, &settings);

    assert!(!output.contains("Shape"), "{output}");
    assert!(output.contains(r#"import { Big } from "./big""#), "{output}");
    assert!(output.contains(r#"import { unused } from "./side-effects""#), "{output}");
    assert!(output.contains(r#"import { used } from "./used""#), "{output}");
}

#[test]
fn without_verbatim_module_syntax_type_only_usages_are_elided() {
    let output = code("/project/imports.ts", IMPORT_SOURCE, &CompilerSettings::default());

    assert!(!output.contains("Shape"), "{output}");
    assert!(!output.contains("./big"), "{output}");
    assert!(!output.contains("./side-effects"), "{output}");
    assert!(output.contains(r#"import { used } from "./used""#), "{output}");
}

#[test]
fn mts_keeps_module_syntax_and_removes_types() {
    let source = r#"
import { readFileSync } from "node:fs";
export const size: number = readFileSync("a").length;
export default function read(path: string): string { return path; }
"#;
    let output = code("/project/reader.mts", source, &CompilerSettings::default());

    assert!(output.contains(r#"import { readFileSync } from "node:fs""#), "{output}");
    assert!(output.contains("export const size = readFileSync"), "{output}");
    assert!(output.contains("export default function read(path)"), "{output}");
    assert!(!output.contains("require("), "{output}");
}

#[test]
fn cts_keeps_commonjs_syntax_and_removes_types() {
    let source = r#"
const fs = require("node:fs") as typeof import("node:fs");
function count(path: string): number { return fs.readFileSync(path).length; }
module.exports = { count };
"#;
    let output = code("/project/counter.cts", source, &CompilerSettings::default());

    assert!(output.contains(r#"const fs = require("node:fs")"#), "{output}");
    assert!(output.contains("function count(path)"), "{output}");
    assert!(output.contains("module.exports = { count }"), "{output}");
    assert!(!output.contains("typeof import"), "{output}");
}

#[test]
fn cts_with_esm_syntax_is_not_converted() {
    let output =
        code("/project/esm.cts", "export const answer: number = 42;", &CompilerSettings::default());
    assert_eq!(output.trim(), "export const answer = 42;");
}
