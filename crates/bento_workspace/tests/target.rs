mod support;

use bento_workspace::{Target, TargetError, select_target};
use support::TempProject;

fn project_with_tasks() -> TempProject {
    let temp = TempProject::new();
    temp.write("package.json", r#"{ "tasks": { "build": "make", "dev": "serve" } }"#);
    temp
}

#[test]
fn explicit_path_selects_the_file() {
    let temp = project_with_tasks();
    temp.write("scripts/build", "");
    temp.write("server.ts", "");

    let nested = select_target("scripts/build", temp.root()).unwrap();
    let dotted = select_target("./server.ts", temp.root()).unwrap();

    assert_eq!(nested.target, Target::Script(temp.path("scripts/build")));
    assert_eq!(dotted.target, Target::Script(temp.path("server.ts")));
    assert_eq!(nested.project.root(), temp.root());
}

#[test]
fn existing_file_with_an_extension_selects_the_file() {
    let temp = project_with_tasks();
    temp.write("server.ts", "");

    let selection = select_target("server.ts", temp.root()).unwrap();

    assert_eq!(selection.target, Target::Script(temp.path("server.ts")));
}

#[test]
fn exact_task_name_beats_a_same_named_relative_file() {
    let temp = project_with_tasks();
    temp.write("build", "");

    let selection = select_target("build", temp.root()).unwrap();

    assert_eq!(selection.target, Target::Task("build".to_string()));
    assert_eq!(selection.project.root(), temp.root());
}

#[test]
fn task_is_found_from_a_subdirectory_of_the_project() {
    let temp = project_with_tasks();
    temp.write("app/routes/home.tsx", "");

    let selection = select_target("dev", &temp.path("app/routes")).unwrap();

    assert_eq!(selection.target, Target::Task("dev".to_string()));
    assert_eq!(selection.project.root(), temp.root());
}

#[test]
fn relative_file_without_a_same_named_task_selects_the_file() {
    let temp = project_with_tasks();
    temp.write("seed", "");

    let selection = select_target("seed", temp.root()).unwrap();

    assert_eq!(selection.target, Target::Script(temp.path("seed")));
}

#[test]
fn unknown_target_is_an_error_rather_than_a_shell_command() {
    let temp = project_with_tasks();

    for argument in ["touch selected-marker", "missing.ts", "deploy"] {
        let error = select_target(argument, temp.root()).unwrap_err();
        assert!(matches!(error, TargetError::Unknown { .. }), "{argument}: {error:?}");
        assert!(error.to_string().contains(argument), "{error}");
    }
    assert!(!temp.path("selected-marker").exists());
}
