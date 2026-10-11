mod support;

use serde_json::json;

use bento_workspace::{PackageManager, PackageType, Project, ProjectError};
use support::{TempProject, fixture_root};

#[test]
fn fixture_manifest_exposes_type_tasks_and_package_manager() {
    let project = Project::for_directory(&fixture_root()).unwrap();
    let manifest = project.manifest();

    assert_eq!(manifest.package_type, Some(PackageType::Module));
    assert_eq!(
        manifest.package_manager,
        Some(PackageManager { name: "pnpm".to_string(), version: Some("12".to_string()) })
    );
    assert_eq!(
        manifest.tasks["dev"],
        json!({ "dependsOn": ["db:migrate"], "command": "bento run --watch server.ts" })
    );
    assert_eq!(manifest.tasks["check"], json!(["oxfmt --check", "oxlint", "tsc"]));
    assert_eq!(manifest.tasks["check:fix"], json!({ "dependsOn": ["fmt", "lint", "typecheck"] }));
    assert_eq!(manifest.tasks["test"], json!("NODE_ENV=test remix test"));
}

#[test]
fn scripts_never_become_tasks() {
    let fixture = Project::for_directory(&fixture_root()).unwrap();
    assert!(!fixture.manifest().tasks.contains_key("predev"));

    let temp = TempProject::new();
    temp.write("package.json", r#"{ "scripts": { "predev": "migrate", "dev": "serve" } }"#);
    let project = Project::for_directory(temp.root()).unwrap();

    assert!(project.manifest().tasks.is_empty());
}

#[test]
fn package_type_and_package_manager_are_optional() {
    let temp = TempProject::new();
    temp.write("package.json", r#"{ "type": "commonjs" }"#);
    let commonjs = Project::for_directory(temp.root()).unwrap();

    assert_eq!(commonjs.manifest().package_type, Some(PackageType::CommonJs));
    assert_eq!(commonjs.manifest().package_manager, None);

    temp.write("package.json", r#"{ "devEngines": { "packageManager": { "name": "npm" } } }"#);
    let untyped = Project::for_directory(temp.root()).unwrap();

    assert_eq!(untyped.manifest().package_type, None);
    assert_eq!(
        untyped.manifest().package_manager,
        Some(PackageManager { name: "npm".to_string(), version: None })
    );
}

#[test]
fn malformed_manifest_names_the_file_and_the_field() {
    let temp = TempProject::new();
    let manifest = temp.write("package.json", r#"{ "tasks": ["dev"] }"#);

    let error = Project::for_directory(temp.root()).unwrap_err();

    assert!(matches!(error, ProjectError::Manifest(_)), "{error:?}");
    let message = error.to_string();
    assert!(message.contains(&manifest.display().to_string()), "{message}");
    assert!(message.contains("tasks"), "{message}");

    temp.write("package.json", "{ not json");
    let syntax = Project::for_directory(temp.root()).unwrap_err().to_string();
    assert!(syntax.contains(&manifest.display().to_string()), "{syntax}");
    assert!(syntax.contains("line 1"), "{syntax}");
}
