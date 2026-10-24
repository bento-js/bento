mod support;

use std::process::Command;

use bento_runtime::{ErrorKind, RunOutcome};
use bento_test_support::{ChildLimit, isolated_fixture_test};

use support::{config, run};

#[test]
fn wrapped_import_failure_is_uncaught_but_original_failure_is_resolution() {
    let root = support::project();
    std::fs::write(root.path().join("package.json"), r#"{"type":"module"}"#).unwrap();
    let source = "let caught;\ntry { await import('./absent.ts'); } catch (error) { caught = error; }\nthrow new Error(`wrapped: ${caught.message}`);\n";
    let RunOutcome::Failed(error) = run(config(root.path()), "wrapped.ts", source) else {
        panic!("wrapped import failure succeeded");
    };
    assert_eq!(error.kind, ErrorKind::Uncaught, "{error:?}");
    assert_eq!(error.location().unwrap().line, 3);

    let RunOutcome::Failed(error) =
        run(config(root.path()), "unresolved.ts", "await import('./absent.ts');")
    else {
        panic!("unresolved import succeeded");
    };
    assert_eq!(error.kind, ErrorKind::Resolution, "{error:?}");
    assert!(!error.message.contains("BENTO_RESOLUTION"), "{error}");
}

#[test]
fn a_new_error_copying_a_module_failure_message_is_uncaught() {
    let root = support::project();
    std::fs::write(root.path().join("package.json"), r#"{"type":"module"}"#).unwrap();
    std::fs::write(root.path().join("invalid.ts"), "const answer: number = ;").unwrap();
    for (name, imported) in [("resolution.ts", "./absent.ts"), ("transform.ts", "./invalid.ts")] {
        let source = format!(
            "let caught;\ntry {{ await import('{imported}'); }} catch (error) {{ caught = error; }}\nthrow new Error(caught.message);\n"
        );
        let RunOutcome::Failed(error) = run(config(root.path()), name, &source) else {
            panic!("{name} succeeded");
        };
        assert_eq!(error.kind, ErrorKind::Uncaught, "{error:?}");
        let location = error.location().expect("new guest exception location");
        assert_eq!(location.path, root.path().join(name).canonicalize().unwrap(), "{error:?}");
        assert_eq!((location.line, location.column), (3, 7), "{error:?}");
    }
}

#[test]
fn guest_syntax_errors_are_uncaught_not_startup_failures() {
    let root = support::project();
    for source in ["JSON.parse('{');", "throw new SyntaxError('guest syntax error');"] {
        let RunOutcome::Failed(error) = run(config(root.path()), "guest.mjs", source) else {
            panic!("guest syntax error succeeded");
        };
        assert_eq!(error.kind, ErrorKind::Uncaught, "{error:?}");
        assert_eq!(error.location().unwrap().line, 1);
    }
}

#[test]
fn unterminated_terminal_sequences_do_not_hide_stacks_causes_or_message_controls() {
    let root = support::project();
    for sequence in ["\\x1b]unfinished", "\\x1bPunfinished", "\\x1b[31mred\\x1b[0m"] {
        let source = format!(
            "throw new Error('tab\\there\\r\\n{sequence}', {{ cause: new Error('retained cause \\x1b\\\\') }});"
        );
        let RunOutcome::Failed(error) = run(config(root.path()), "controls.mjs", &source) else {
            panic!("guest exception succeeded");
        };
        assert!(!error.message.contains('\u{1b}'), "{error:?}");
        assert!(error.message.contains("tab\there\r\n"), "{error:?}");
        assert!(
            error.message.contains("Caused by:") && error.message.contains("retained cause"),
            "{error:?}"
        );
        assert!(error.message.contains("controls.mjs:1:"), "{error:?}");
    }
}

#[test]
fn module_failure_messages_have_no_internal_markers_and_offer_relevant_hints() {
    let root = support::project();
    for (name, source, kind, hint, excluded) in [
        (
            "host.mjs",
            "import 'host:unregistered';",
            ErrorKind::Resolution,
            "host did not register",
            "bento install",
        ),
        (
            "npm.mjs",
            "import 'missing-bento-package';",
            ErrorKind::Resolution,
            "bento install",
            "BENTO_RESOLUTION",
        ),
        (
            "syntax.ts",
            "const answer: number = ;",
            ErrorKind::Transform,
            "syntax.ts",
            "BENTO_TRANSFORM",
        ),
        ("syntax.cjs", "const answer = ;", ErrorKind::Startup, "syntax.cjs", "BENTO_PARSE"),
    ] {
        let RunOutcome::Failed(error) = run(config(root.path()), name, source) else {
            panic!("{name} succeeded");
        };
        assert_eq!(error.kind, kind, "{error:?}");
        assert!(error.message.contains(hint), "{error}");
        assert!(!error.message.contains(excluded), "{error}");
    }
}

#[test]
fn runtime_messages_are_plain_even_when_terminal_color_is_enabled() {
    const NAME: &str = "runtime_messages_are_plain_even_when_terminal_color_is_enabled";
    if std::env::var("BENTO_COLOR_TEST").as_deref() != Ok(NAME) {
        for (force_color, no_color) in [("1", ""), ("", "1")] {
            let child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--nocapture"])
                .env("BENTO_COLOR_TEST", NAME)
                .env("TERM", "xterm-256color")
                .env("FORCE_COLOR", force_color)
                .env("NO_COLOR", no_color)
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&child.stdout);
            let stderr = String::from_utf8_lossy(&child.stderr);
            assert!(child.status.success(), "{stdout}\n{stderr}");
            assert!(
                stdout.lines().any(|line| line.starts_with("test result: ok. 1 passed;")),
                "color child did not run exactly one test:\n{stdout}"
            );
        }
        return;
    }
    let root = support::project();
    for (name, source) in [
        ("throw.ts", "throw new Error('guest failure');"),
        ("syntax.ts", "const answer: number = ;"),
        ("missing.mjs", "import 'missing-bento-package';"),
        ("permission.mjs", "import { readFileSync } from 'node:fs'; readFileSync('/denied');"),
        ("guest-ansi.mjs", "throw new Error('\\x1b[31mcolored guest text\\x1b[0m');"),
    ] {
        let RunOutcome::Failed(error) = run(config(root.path()), name, source) else {
            panic!("{name} succeeded");
        };
        assert!(!error.message.contains('\u{1b}'), "{error:?}");
    }
}

#[test]
fn fixture_reexecution_rejects_a_filter_that_runs_no_tests() {
    let rejected = std::panic::catch_unwind(|| {
        isolated_fixture_test("nonexistent_bento_fixture_test", ChildLimit::Unbounded)
    });
    let panic = rejected.expect_err("fixture helper accepted a child that ran zero tests");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("fixture helper panicked without a message");
    assert!(message.contains("fixture child did not run exactly one test:"), "{message}");
}

#[test]
fn addon_denial_names_bento_authority_and_the_loading_package_frame() {
    let root = support::project();
    let package = root.path().join("node_modules/local-addon");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(package.join("package.json"), r#"{"main":"index.cjs"}"#).unwrap();
    std::fs::write(package.join("binding.node"), []).unwrap();
    std::fs::write(package.join("index.cjs"), "\n\nmodule.exports = require('./binding.node');\n")
        .unwrap();
    let RunOutcome::Failed(error) = run(
        config(root.path()),
        "addon.mjs",
        "import { createRequire } from 'node:module';\nconst require = createRequire(import.meta.url);\nrequire('local-addon');\n",
    ) else {
        panic!("addon was not denied");
    };
    assert_eq!(error.kind, ErrorKind::Permission, "{error:?}");
    assert!(!error.message.contains("--allow-ffi"), "{error}");
    assert!(
        error.message.contains("addons") && error.message.contains("--allow-addons"),
        "{error}"
    );
    let location = error.location().expect("addon load location");
    assert_eq!(location.path, package.join("index.cjs").canonicalize().unwrap(), "{error:?}");
    assert_eq!(location.line, 3, "{error:?}");
}

#[test]
fn require_source_syntax_failures_remain_startup_errors() {
    let root = support::project();
    std::fs::write(root.path().join("invalid.cjs"), "const answer = ;").unwrap();
    let RunOutcome::Failed(error) = run(
        config(root.path()),
        "require.mjs",
        "import { createRequire } from 'node:module';\nconst require = createRequire(import.meta.url);\nrequire('./invalid.cjs');",
    ) else {
        panic!("invalid required source executed");
    };
    assert_eq!(error.kind, ErrorKind::Startup, "{error:?}");
}
