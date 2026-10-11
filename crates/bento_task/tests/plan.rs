mod support;

use serde_json::{Value, json};

use bento_task::{Plan, PlanError};
use bento_workspace::{Manifest, Project};
use support::{TempProject, fixture_root};

fn manifest(tasks: Value) -> Manifest {
    let Value::Object(tasks) = tasks else { panic!("tasks must be an object") };
    Manifest { package_type: None, package_manager: None, tasks: tasks.into_iter().collect() }
}

fn steps(plan: &Plan) -> Vec<(&str, &str)> {
    plan.steps().iter().map(|step| (step.task.as_str(), step.command.as_str())).collect()
}

fn no_arguments() -> Vec<String> {
    Vec::new()
}

#[test]
fn fixture_check_fix_runs_fmt_lint_and_typecheck_in_order() {
    let fixture = Project::for_directory(&fixture_root()).unwrap();

    let plan = Plan::new(fixture.manifest(), "check:fix", &no_arguments()).unwrap();

    assert_eq!(steps(&plan), [("fmt", "oxfmt"), ("lint", "oxlint --fix"), ("typecheck", "tsc")]);
}

#[test]
fn fixture_dev_migrates_once_then_runs_its_command_without_predev() {
    let fixture = Project::for_directory(&fixture_root()).unwrap();

    let plan = Plan::new(fixture.manifest(), "dev", &no_arguments()).unwrap();

    assert_eq!(
        steps(&plan),
        [("db:migrate", "remix db migrate"), ("dev", "bento run --watch server.ts")]
    );
}

#[test]
fn dependencies_run_depth_first_in_declared_order_and_shared_ones_once() {
    let tasks = manifest(json!({
        "all": { "dependsOn": ["build", "test"], "command": "echo all" },
        "build": { "dependsOn": ["setup"], "command": "echo build" },
        "test": { "dependsOn": ["setup", "lint"], "command": "echo test" },
        "lint": "echo lint",
        "setup": ["echo setup one", "echo setup two"],
    }));

    let plan = Plan::new(&tasks, "all", &no_arguments()).unwrap();

    assert_eq!(
        steps(&plan),
        [
            ("setup", "echo setup one"),
            ("setup", "echo setup two"),
            ("build", "echo build"),
            ("lint", "echo lint"),
            ("test", "echo test"),
            ("all", "echo all"),
        ]
    );
}

#[test]
fn invalid_task_shapes_are_rejected() {
    let shapes = [
        json!(""),
        json!("   "),
        json!([]),
        json!([1]),
        json!(["echo ok", ""]),
        json!({}),
        json!({ "dependsOn": [] }),
        json!({ "command": 1 }),
        json!({ "command": "" }),
        json!({ "command": ["echo a", "echo b"] }),
        json!({ "dependsOn": "setup" }),
        json!({ "dependsOn": [""] }),
        json!({ "dependsOn": [3] }),
        json!({ "command": "echo ok", "depends_on": ["setup"] }),
        json!(42),
        json!(true),
        json!(null),
    ];
    for shape in shapes {
        let tasks = manifest(json!({ "broken": shape, "setup": "echo setup" }));

        let error = Plan::new(&tasks, "broken", &no_arguments()).unwrap_err();

        assert!(
            matches!(&error, PlanError::InvalidTask { task, .. } if task == "broken"),
            "{shape}: {error:?}"
        );
        assert!(error.to_string().contains("broken"), "{error}");
    }
}

#[test]
fn invalid_dependency_is_reported_for_the_selected_graph_only() {
    let tasks = manifest(json!({
        "build": { "dependsOn": ["setup"], "command": "echo build" },
        "setup": 3,
        "lint": "echo lint",
    }));

    let error = Plan::new(&tasks, "build", &no_arguments()).unwrap_err();
    let unrelated = Plan::new(&tasks, "lint", &no_arguments()).unwrap();

    assert!(matches!(&error, PlanError::InvalidTask { task, .. } if task == "setup"), "{error:?}");
    assert_eq!(steps(&unrelated), [("lint", "echo lint")]);
}

#[test]
fn missing_dependency_is_reported() {
    let tasks = manifest(json!({ "dev": { "dependsOn": ["db:migrate"], "command": "serve" } }));

    let error = Plan::new(&tasks, "dev", &no_arguments()).unwrap_err();

    assert_eq!(
        error,
        PlanError::MissingDependency {
            task: "dev".to_string(),
            dependency: "db:migrate".to_string()
        }
    );
    assert!(error.to_string().contains("db:migrate"), "{error}");
}

#[test]
fn cycles_are_reported_with_their_path() {
    let tasks = manifest(json!({
        "a": { "dependsOn": ["b"], "command": "echo a" },
        "b": { "dependsOn": ["c"] },
        "c": { "dependsOn": ["a"], "command": "echo c" },
        "self": { "dependsOn": ["self"], "command": "echo self" },
    }));

    let cycle = Plan::new(&tasks, "a", &no_arguments()).unwrap_err();
    let own = Plan::new(&tasks, "self", &no_arguments()).unwrap_err();

    assert_eq!(cycle, PlanError::Cycle { tasks: ["a", "b", "c", "a"].map(String::from).to_vec() });
    assert!(cycle.to_string().contains("a → b → c → a"), "{cycle}");
    assert_eq!(own, PlanError::Cycle { tasks: ["self", "self"].map(String::from).to_vec() });
}

#[test]
fn unknown_task_is_reported() {
    let tasks = manifest(json!({ "dev": "serve" }));

    let error = Plan::new(&tasks, "deploy", &no_arguments()).unwrap_err();

    assert_eq!(error, PlanError::UnknownTask { task: "deploy".to_string() });
}

#[test]
fn arguments_are_rejected_for_dependency_only_and_multi_command_tasks() {
    let tasks = manifest(json!({
        "check:fix": { "dependsOn": ["fmt"] },
        "check": ["oxfmt --check", "oxlint"],
        "fmt": "oxfmt",
    }));
    let arguments = vec!["--verbose".to_string()];

    let dependency_only = Plan::new(&tasks, "check:fix", &arguments).unwrap_err();
    let multi_command = Plan::new(&tasks, "check", &arguments).unwrap_err();

    assert!(
        matches!(&dependency_only, PlanError::ArgumentsWithoutCommand { task } if task == "check:fix"),
        "{dependency_only:?}"
    );
    assert!(
        matches!(&multi_command, PlanError::ArgumentsWithSeveralCommands { task } if task == "check"),
        "{multi_command:?}"
    );
}

#[test]
fn plan_errors_arrive_before_any_command_runs() {
    let temp = TempProject::with_tasks(json!({}));
    let marker = temp.path("setup-ran");
    let tasks = manifest(json!({
        "setup": format!("touch {}", marker.display()),
        "build": { "dependsOn": ["setup", "codegen"], "command": "echo build" },
    }));

    let error = Plan::new(&tasks, "build", &no_arguments()).unwrap_err();

    assert!(matches!(error, PlanError::MissingDependency { .. }), "{error:?}");
    assert!(!marker.exists());
}
