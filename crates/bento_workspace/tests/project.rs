mod support;

use bento_workspace::{Project, ProjectError};
use support::TempProject;

#[test]
fn script_project_is_the_nearest_ancestor_with_a_package_json() {
    let temp = TempProject::new();
    temp.write("package.json", "{}");
    temp.write("packages/app/package.json", r#"{ "type": "module" }"#);
    let script = temp.write("packages/app/src/server.ts", "");

    let project = Project::for_script(&script).unwrap();

    assert_eq!(project.root(), temp.path("packages/app"));
}

#[test]
fn directory_project_is_the_nearest_ancestor_with_a_package_json() {
    let temp = TempProject::new();
    temp.write("package.json", "{}");
    temp.write("src/deep/notes.txt", "");

    let from_nested = Project::for_directory(&temp.path("src/deep")).unwrap();
    let from_root = Project::for_directory(temp.root()).unwrap();

    assert_eq!(from_nested.root(), temp.root());
    assert_eq!(from_root.root(), temp.root());
}

#[test]
fn discovery_fails_when_no_ancestor_has_a_package_json() {
    let temp = TempProject::new();
    let script = temp.write("src/server.ts", "");

    let from_directory = Project::for_directory(&temp.path("src")).unwrap_err();
    let from_script = Project::for_script(&script).unwrap_err();

    assert!(matches!(from_directory, ProjectError::NotFound(_)), "{from_directory:?}");
    assert!(matches!(from_script, ProjectError::NotFound(_)), "{from_script:?}");
    assert!(from_directory.to_string().contains("package.json"), "{from_directory}");
}

#[test]
fn missing_script_is_reported_instead_of_discovering_a_project() {
    let temp = TempProject::new();
    temp.write("package.json", "{}");

    let error = Project::for_script(&temp.path("missing.ts")).unwrap_err();

    assert!(matches!(error, ProjectError::MissingScript(_)), "{error:?}");
    assert!(error.to_string().contains("missing.ts"), "{error}");
}
