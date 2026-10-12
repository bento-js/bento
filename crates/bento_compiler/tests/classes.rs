mod common;

use std::path::Path;

use bento_compiler::{CompilerSettings, transform};
use common::code;

const DECORATED: &str = r#"
function sealed(target: Function) {}
function log(target: object, key: string) {}

@sealed
export class Service {
    @log
    run(input: string): void {}
}
"#;

#[test]
fn experimental_decorators_transform_legacy_decorators() {
    let settings =
        CompilerSettings { experimental_decorators: true, ..CompilerSettings::default() };
    let output = code("/project/src/service.ts", DECORATED, &settings);

    assert!(!output.contains("@sealed"), "{output}");
    assert!(!output.contains("@log"), "{output}");
    assert!(output.contains("_decorate([sealed], Service)"), "{output}");
    assert!(output.contains(r#"_decorate([log], Service.prototype, "run", null)"#), "{output}");
    assert!(output.contains(r#"from "@oxc-project/runtime/helpers/decorate""#), "{output}");
}

#[test]
fn emit_decorator_metadata_adds_design_metadata() {
    let settings = CompilerSettings {
        experimental_decorators: true,
        emit_decorator_metadata: true,
        ..CompilerSettings::default()
    };
    let output = code("/project/src/service.ts", DECORATED, &settings);
    assert!(output.contains(r#"_decorateMetadata("design:paramtypes", [String])"#), "{output}");
}

#[test]
fn standard_decorators_without_experimental_decorators_are_a_clear_error() {
    let error =
        transform(Path::new("/project/src/service.ts"), DECORATED, &CompilerSettings::default())
            .expect_err("standard decorators cannot execute");
    let message = error.to_string();
    assert!(message.starts_with("/project/src/service.ts:5:1: "), "{message}");
    assert!(message.contains("experimentalDecorators"), "{message}");
}

const FIELDS: &str = r#"
export class Widget extends Base {
    declared: number;
    label = "widget";
    #secret = 1;
    static count = 0;
    constructor() {
        super();
    }
}
"#;

#[test]
fn use_define_for_class_fields_true_keeps_field_definitions() {
    let output = code("/project/src/widget.ts", FIELDS, &CompilerSettings::default());

    assert!(output.contains("declared;"), "{output}");
    assert!(output.contains(r#"label = "widget";"#), "{output}");
    assert!(output.contains("#secret = 1;"), "{output}");
    assert!(output.contains("static count = 0;"), "{output}");
    assert!(!output.contains("this.label"), "{output}");
}

#[test]
fn use_define_for_class_fields_false_assigns_in_the_constructor() {
    let settings =
        CompilerSettings { use_define_for_class_fields: false, ..CompilerSettings::default() };
    let output = code("/project/src/widget.ts", FIELDS, &settings);

    assert!(!output.contains("declared"), "{output}");
    assert!(output.contains(r#"this.label = "widget";"#), "{output}");
    assert!(output.contains("#secret"), "{output}");
    assert!(!output.contains("WeakMap"), "{output}");
    assert!(!output.contains("@oxc-project/runtime"), "{output}");
    let super_call = output.find("super();").unwrap();
    let assignment = output.find("this.label").unwrap();
    assert!(assignment > super_call, "{output}");
}
