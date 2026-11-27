mod support;

use bento_workspace::{
    ManifestError, ManifestProblem, PackageManager, PackageType, Project, ProjectError,
};
use support::{TempProject, fixture_root};

#[test]
fn fixture_manifest_exposes_type_and_package_manager() {
    let project = Project::for_directory(&fixture_root()).unwrap();
    let manifest = project.manifest();

    assert_eq!(manifest.package_type, Some(PackageType::Module));
    assert_eq!(
        manifest.package_manager,
        Some(PackageManager { name: "pnpm".to_string(), version: Some("12".to_string()) })
    );
}

#[test]
fn a_tasks_field_of_any_json_type_is_ignored() {
    let temp = TempProject::new();

    for tasks in ["null", "true", "1", r#""dev""#, r#"["dev"]"#, r#"{ "dev": "serve" }"#] {
        temp.write("package.json", &format!(r#"{{ "type": "module", "tasks": {tasks} }}"#));

        let project = Project::for_directory(temp.root())
            .unwrap_or_else(|error| panic!("`tasks: {tasks}`: {error}"));

        assert_eq!(project.manifest().package_type, Some(PackageType::Module), "{tasks}");
    }
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
    let manifest = temp.write("package.json", r#"{ "devEngines": { "packageManager": "pnpm" } }"#);

    let error = Project::for_directory(temp.root()).unwrap_err();

    assert!(matches!(error, ProjectError::Manifest(_)), "{error:?}");
    let message = error.to_string();
    assert!(message.contains(&manifest.display().to_string()), "{message}");
    assert!(message.contains("devEngines.packageManager"), "{message}");

    temp.write("package.json", "{ not json");
    let syntax = Project::for_directory(temp.root()).unwrap_err().to_string();
    assert!(syntax.contains(&manifest.display().to_string()), "{syntax}");
    assert!(syntax.contains("line 1"), "{syntax}");
}

#[test]
fn an_unknown_package_type_is_reported_as_a_problem_with_that_field() {
    let temp = TempProject::new();
    temp.write("package.json", r#"{ "type": "esm" }"#);

    let error = Project::for_directory(temp.root()).unwrap_err();

    let ProjectError::Manifest(ManifestError::Invalid { problem, .. }) = error else {
        panic!("{error:?}");
    };
    assert_eq!(problem, ManifestProblem::InvalidPackageType);
}
