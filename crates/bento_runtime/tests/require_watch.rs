mod support;

use std::path::{Path, PathBuf};

use bento_runtime::{RunOutcome, start};
use support::{completed, config, project, run};

fn watch_inputs(app: &Path, entry: &str) -> Vec<PathBuf> {
    let mut settings = config(app);
    settings.grants.read = bento_host::Scope::All;
    let runtime = start(settings, &app.join(entry)).unwrap();
    let handle = runtime.handle();
    completed(runtime.wait());
    handle.watch_inputs()
}

#[test]
fn proposal_0001_dependency_probes_ignore_application_anchored_require_parents() {
    for entry in ["main.cjs", "main.mjs"] {
        for anchor in ["noop.js", entry] {
            for resolve in [
                "createRequire(anchor).resolve(target)",
                "Module._resolveFilename(target, { id: anchor, filename: anchor, paths: Module._nodeModulePaths(path.dirname(anchor)) })",
                "createRequire(anchor)(target)",
            ] {
                let root = project();
                let app = root.path().join("app");
                let dependency = app.join("node_modules/config-loader");
                std::fs::create_dir_all(&dependency).unwrap();
                std::fs::write(app.join("package.json"), "{}").unwrap();
                std::fs::write(
                    dependency.join("package.json"),
                    r#"{"name":"config-loader","main":"index.cjs"}"#,
                )
                .unwrap();
                std::fs::write(
                    dependency.join("index.cjs"),
                    format!(
                        "const path = require('node:path'); const {{ createRequire, Module }} = require('node:module'); module.exports = (start) => {{ const anchor = path.join(start, {anchor:?}); for (let dir = start; ; dir = path.dirname(dir)) {{ const target = path.join(dir, 'probe.config.js'); try {{ {resolve}; }} catch {{}} if (dir === path.dirname(dir)) break; }} try {{ const target = '../relative.config.js'; {resolve}; }} catch {{}} }};"
                    ),
                )
                .unwrap();
                std::fs::write(
                    app.join(entry),
                    if entry.ends_with(".cjs") {
                        "require('config-loader')(__dirname);"
                    } else {
                        "import probe from 'config-loader'; probe(import.meta.dirname);"
                    },
                )
                .unwrap();

                let recorded = watch_inputs(&app, entry);
                let base = root.path().canonicalize().unwrap();
                let expected = vec![base.join("app").join(entry), base.join("app/package.json")];
                assert_eq!(recorded, expected, "{entry}, {anchor}, {resolve}");
            }
        }
    }
}

#[test]
fn proposal_0001_synthetic_require_parents_do_not_record_failed_targets() {
    for anchor in ["noop.js", "main.cjs"] {
        for resolve in [
            "createRequire(anchor).resolve('../shared/missing.cjs')",
            "Module._resolveFilename('../shared/missing.cjs', { id: anchor, filename: anchor, paths: Module._nodeModulePaths(path.dirname(anchor)) })",
            "createRequire(anchor)('../shared/missing.cjs')",
        ] {
            let root = project();
            let app = root.path().join("app");
            std::fs::create_dir(&app).unwrap();
            std::fs::write(app.join("package.json"), "{}").unwrap();
            std::fs::write(
            app.join("main.cjs"),
            format!("const path = require('node:path'); const {{ createRequire, Module }} = require('node:module'); const anchor = path.join(__dirname, {anchor:?}); try {{ {resolve}; }} catch {{}}"),
        )
        .unwrap();

            let recorded = watch_inputs(&app, "main.cjs");
            let base = root.path().canonicalize().unwrap();
            assert_eq!(
                recorded,
                vec![base.join("app/main.cjs"), base.join("app/package.json")],
                "{anchor}, {resolve}"
            );
        }
    }
}

#[test]
fn proposal_0001_source_loaded_after_a_failed_require_records_its_package_scope() {
    let root = project();
    let app = root.path().join("app");
    std::fs::create_dir(&app).unwrap();
    std::fs::create_dir(root.path().join("shared")).unwrap();
    std::fs::write(app.join("package.json"), "{}").unwrap();
    std::fs::write(root.path().join("shared/package.json"), "{}").unwrap();
    std::fs::write(
        app.join("main.cjs"),
        "try { require('../shared/later.cjs'); } catch {} require('node:fs').writeFileSync(require('node:path').join(__dirname, '../shared/later.cjs'), 'module.exports = 42;'); require('../shared/later.cjs');",
    )
    .unwrap();
    let mut settings = config(&app);
    settings.grants.read = bento_host::Scope::All;
    settings.grants.write = bento_host::Scope::All;
    let runtime = start(settings, &app.join("main.cjs")).unwrap();
    let handle = runtime.handle();
    assert!(matches!(runtime.wait(), RunOutcome::Completed));
    let recorded = handle.watch_inputs();
    let scope = root.path().canonicalize().unwrap().join("shared/package.json");
    assert!(recorded.contains(&scope), "{recorded:?}");
}

fn wrapped_require_inputs(hook: &str) -> (tempfile::TempDir, Vec<PathBuf>) {
    let root = project();
    let app = root.path().join("app");
    let dependency = app.join("node_modules/instrumentation");
    std::fs::create_dir_all(&dependency).unwrap();
    std::fs::write(app.join("package.json"), "{}").unwrap();
    std::fs::write(
        dependency.join("package.json"),
        r#"{"name":"instrumentation","main":"index.cjs"}"#,
    )
    .unwrap();
    std::fs::write(
        dependency.join("index.cjs"),
        format!("const Module = require('node:module'); const original = {hook}; {hook} = function (...args) {{ return original.apply(this, args); }};"),
    ).unwrap();
    std::fs::write(
        app.join("main.cjs"),
        "require('instrumentation'); try { require('../shared/missing.cjs'); } catch {}",
    )
    .unwrap();
    let recorded = watch_inputs(&app, "main.cjs");
    std::fs::create_dir(root.path().join("shared")).unwrap();
    std::fs::write(root.path().join("shared/missing.cjs"), "module.exports = 42;").unwrap();
    let mut settings = config(&app);
    settings.grants.read = bento_host::Scope::All;
    completed(run(
        settings,
        "main.cjs",
        "require('instrumentation'); require('../shared/missing.cjs');",
    ));
    (root, recorded)
}

#[test]
fn proposal_0001_wrapped_require_retains_the_application_failed_target() {
    let (root, recorded) = wrapped_require_inputs("Module.prototype.require");
    let target = root.path().canonicalize().unwrap().join("shared/missing.cjs");
    assert!(recorded.contains(&target), "{recorded:?}");
}

#[test]
fn proposal_0001_wrapped_resolve_filename_retains_the_application_failed_target() {
    let (root, recorded) = wrapped_require_inputs("Module._resolveFilename");
    let target = root.path().canonicalize().unwrap().join("shared/missing.cjs");
    assert!(recorded.contains(&target), "{recorded:?}");
}

#[test]
fn proposal_0001_failed_require_of_esm_discards_successful_nested_resolution_candidates() {
    let root = project();
    let library = project();
    let app = root.path().join("app");
    std::fs::create_dir(&app).unwrap();
    std::fs::create_dir(root.path().join("shared")).unwrap();
    std::fs::write(app.join("package.json"), "{}").unwrap();
    std::fs::create_dir(library.path().join("lib")).unwrap();
    std::fs::write(library.path().join("lib/index.js"), "module.exports = 'LIB INDEX';").unwrap();
    std::fs::write(
        root.path().join("shared/cjs.cjs"),
        format!("module.exports = require({:?});", library.path().join("lib").to_str().unwrap()),
    )
    .unwrap();
    std::fs::write(
        root.path().join("shared/esm.mjs"),
        "import value from './cjs.cjs'; throw new Error('boom');",
    )
    .unwrap();
    std::fs::write(app.join("main.cjs"), "try { require('../shared/esm.mjs'); } catch {}").unwrap();

    let recorded = watch_inputs(&app, "main.cjs");
    let base = root.path().canonicalize().unwrap();
    let mut expected: Vec<_> = [
        "app/main.cjs",
        "app/package.json",
        "shared/cjs.cjs",
        "shared/esm.mjs",
        "shared/package.json",
    ]
    .map(|path| base.join(path))
    .into();
    let library = library.path().canonicalize().unwrap();
    expected.extend([library.join("lib/index.js"), library.join("lib/package.json")]);
    expected.sort();
    assert_eq!(recorded, expected);
}

#[test]
fn proposal_0001_successful_custom_require_resolution_discards_probes_when_loading_fails() {
    for resolution in [
        "require.extensions['.txt'] = (m, f) => { m.exports = fs.readFileSync(f + '.absent', 'utf8'); };",
        "require('node:module').registerHooks({ resolve(request, context, next) { if (request === '../shared/data') return { url: new URL('../shared/data.txt', require('node:url').pathToFileURL(__filename)).href, shortCircuit: true }; return next(request, context); }, load(url, context, next) { if (url.endsWith('/data.txt')) fs.readFileSync(require('node:url').fileURLToPath(url) + '.absent'); return next(url, context); } });",
    ] {
        let root = project();
        let app = root.path().join("app");
        std::fs::create_dir(&app).unwrap();
        std::fs::create_dir(root.path().join("shared")).unwrap();
        std::fs::write(app.join("package.json"), "{}").unwrap();
        std::fs::write(root.path().join("shared/data.txt"), "existing target").unwrap();
        std::fs::write(
            app.join("main.cjs"),
            format!("const fs = require('node:fs'); {resolution} try {{ require('../shared/data'); }} catch (error) {{ require('node:assert/strict').equal(error.code, 'ENOENT'); }}"),
        )
        .unwrap();

        let recorded = watch_inputs(&app, "main.cjs");
        let shared = root.path().canonicalize().unwrap().join("shared");
        for candidate in ["data", "data.js", "data.json", "data/index.js", "data.txt.absent"] {
            assert!(!recorded.contains(&shared.join(candidate)), "{resolution}: {recorded:?}");
        }
    }
}

#[test]
fn proposal_0001_missing_custom_require_extensions_are_retained() {
    let root = project();
    let app = root.path().join("app");
    std::fs::create_dir(&app).unwrap();
    std::fs::write(app.join("package.json"), "{}").unwrap();
    std::fs::write(
        app.join("main.cjs"),
        "require.extensions['.ts'] = (m, f) => { m.exports = require('node:fs').readFileSync(f, 'utf8'); }; try { require('../shared/missing'); } catch {}",
    )
    .unwrap();

    let recorded = watch_inputs(&app, "main.cjs");
    let target = root.path().canonicalize().unwrap().join("shared/missing.ts");
    assert!(recorded.contains(&target), "{recorded:?}");
}

#[test]
fn proposal_0001_load_failure_does_not_repeat_registered_resolution_hooks() {
    let root = project();
    let app = root.path().join("app");
    std::fs::create_dir(&app).unwrap();
    std::fs::create_dir(root.path().join("shared")).unwrap();
    std::fs::write(app.join("package.json"), "{}").unwrap();
    std::fs::write(root.path().join("shared/data.txt"), "existing target").unwrap();
    std::fs::write(
        app.join("main.cjs"),
        "const fs = require('node:fs'); const assert = require('node:assert/strict'); let resolutions = 0; require.extensions['.txt'] = (m, f) => { m.exports = fs.readFileSync(f + '.absent', 'utf8'); }; require('node:module').registerHooks({ resolve(request, context, next) { if (request === '../shared/data') resolutions++; return next(request, context); } }); try { require('../shared/data'); } catch (error) { assert.equal(error.code, 'ENOENT'); } assert.equal(resolutions, 1);",
    )
    .unwrap();

    watch_inputs(&app, "main.cjs");
}

#[test]
fn proposal_0001_dependency_calls_borrowing_cached_application_parents_are_attributable() {
    for require in [
        "require.main.require(target)",
        "module.parent.require(target)",
        "require('node:module').Module._load(target, module.parent)",
    ] {
        let root = project();
        let app = root.path().join("app");
        let dependency = app.join("node_modules/config-loader");
        std::fs::create_dir_all(&dependency).unwrap();
        std::fs::write(app.join("package.json"), "{}").unwrap();
        std::fs::write(
            dependency.join("package.json"),
            r#"{"name":"config-loader","main":"index.cjs"}"#,
        )
        .unwrap();
        let filename =
            format!("{}.probe.config.js", root.path().file_name().unwrap().to_str().unwrap());
        std::fs::write(
            dependency.join("index.cjs"),
            format!("const path = require('node:path'); for (let directory = path.dirname(module.parent.filename); ; directory = path.dirname(directory)) {{ const target = path.join(directory, {filename:?}); try {{ {require}; }} catch {{}} if (directory === path.dirname(directory)) break; }}"),
        )
        .unwrap();
        std::fs::write(app.join("main.cjs"), "require('config-loader');").unwrap();

        let recorded = watch_inputs(&app, "main.cjs");
        for ancestor in app.canonicalize().unwrap().ancestors() {
            assert!(recorded.contains(&ancestor.join(&filename)), "{require}: {recorded:?}");
        }
    }
}

#[test]
fn proposal_0001_require_resolve_with_search_paths_records_no_failed_targets() {
    for paths in ["[path.join(__dirname, '../other/sub')]", "[]"] {
        let root = project();
        let app = root.path().join("app");
        std::fs::create_dir(&app).unwrap();
        std::fs::create_dir_all(root.path().join("other/sub")).unwrap();
        std::fs::write(app.join("package.json"), "{}").unwrap();
        std::fs::write(
            app.join("main.cjs"),
            format!("const path = require('node:path'); try {{ require.resolve('../phantom', {{ paths: {paths} }}); }} catch {{}}"),
        )
        .unwrap();

        let recorded = watch_inputs(&app, "main.cjs");
        let base = root.path().canonicalize().unwrap();
        assert_eq!(
            recorded,
            vec![base.join("app/main.cjs"), base.join("app/package.json")],
            "{paths}"
        );
    }
}
