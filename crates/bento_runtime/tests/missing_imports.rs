mod support;

use bento_runtime::{ErrorKind, RunOutcome, start};
use support::{completed, config, project, run};

#[test]
fn failed_relative_and_absolute_imports_remain_watch_inputs() {
    let root = project();
    std::fs::create_dir_all(root.path().join("app")).unwrap();
    std::fs::create_dir(root.path().join("sibling")).unwrap();
    let app = root.path().join("app");
    let expected = root.path().canonicalize().unwrap().join("sibling/missing.mjs");
    for specifier in ["../sibling/missing.mjs".to_owned(), expected.to_string_lossy().into_owned()]
    {
        let entry = app.join("entry.mjs");
        std::fs::write(&entry, format!("import {specifier:?};")).unwrap();
        let mut settings = config(&app);
        settings.grants.read = bento_host::Scope::All;
        let runtime = start(settings, &entry).unwrap();
        let handle = runtime.handle();
        let outcome = runtime.wait();
        assert!(
            matches!(outcome, RunOutcome::Failed(error) if error.kind == ErrorKind::Resolution)
        );
        assert!(
            handle.loaded_modules().contains(&expected),
            "missing candidate not recorded: {:?}",
            handle.loaded_modules()
        );
    }
    std::fs::write(&expected, "export const repaired = true;").unwrap();
    let mut settings = config(&app);
    settings.grants.read = bento_host::Scope::All;
    completed(run(settings, "repaired.mjs", "import '../sibling/missing.mjs';"));
}
