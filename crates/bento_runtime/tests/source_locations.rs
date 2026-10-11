mod support;

use bento_runtime::{ErrorKind, RunOutcome};
use support::{config, run};

#[test]
fn typescript_syntax_error_uses_original_diagnostic_position() {
    assert_location("syntax.ts", "\n\nconst value: number = ;\n", ErrorKind::Transform, 3, 23);
}

#[test]
fn typescript_throw_and_rejection_use_original_source_maps() {
    for (name, statement) in [
        ("throw.ts", "throw new Error('original throw');"),
        ("rejection.ts", "Promise.reject(new Error('original rejection'));"),
    ] {
        let source = format!(
            "enum Number {{ Answer = 42 }}\n\nconst answer: number = Number.Answer;\n\n\n{statement}\n"
        );
        let column = if name == "throw.ts" { 7 } else { 16 };
        assert_location(name, &source, ErrorKind::Uncaught, 6, column);
    }
}

#[test]
fn unresolved_import_points_to_the_importing_typescript_literal() {
    assert_location(
        "missing.ts",
        "enum Number { Answer = 42 }\n\nimport answer from './absent.ts';\nconsole.log(answer);\n",
        ErrorKind::Resolution,
        3,
        20,
    );
}

#[test]
fn a_new_exception_embedding_a_caught_import_error_uses_its_own_location() {
    let root = support::project();
    std::fs::write(root.path().join("package.json"), r#"{"type":"module"}"#).unwrap();
    let source = "let caught;\ntry { await import('./absent.ts'); } catch (error) { caught = error; }\nthrow new Error(`New exception: ${caught.message}`);\n";
    let RunOutcome::Failed(error) = run(config(root.path()), "caught.ts", source) else {
        panic!("guest succeeded");
    };
    let location = error.location().expect("new exception location");
    assert_eq!(location.path, root.path().join("caught.ts").canonicalize().unwrap());
    assert_eq!((location.line, location.column), (3, 7), "{error:?}");
}

fn assert_location(name: &str, source: &str, kind: ErrorKind, line: u32, column: u32) {
    let root = support::project();
    std::fs::write(root.path().join("package.json"), r#"{"type":"module"}"#).unwrap();
    let RunOutcome::Failed(error) = run(config(root.path()), name, source) else {
        panic!("guest succeeded");
    };
    assert_eq!(error.kind, kind, "{error:?}");
    let location = error.location().expect("structured source location");
    assert_eq!(location.path, root.path().join(name).canonicalize().unwrap());
    assert_eq!((location.line, location.column), (line, column), "{error:?}");
    assert!(error.message.contains(name));
}
