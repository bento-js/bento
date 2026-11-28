//! Bento's permission model measured against Node. Each case runs the same command line and
//! script under `bento run` and under the pinned Node release, then compares what the script
//! reports: whether the command line is accepted, what `process.permission.has()` answers, and
//! the shape of every denial. Node 26.13.1 is the reference, and Node v27.0.0-alpha.0.2.0 for
//! the environment. `mise run rust` puts both on `PATH`; the cases find each by its version.
//!
//! The project lives at its canonical path and both runtimes run in it, so Node's
//! compare-as-written paths and Bento's real locations agree except where a case says otherwise.

use std::path::{Path, PathBuf};
use std::process::Command;

const BENTO: &str = env!("CARGO_BIN_EXE_bento");
const NODE: &str = "v26.13.1";
const NODE_ENVIRONMENT: &str = "v27.0.0-alpha.0.2.0";

/// Prints one JSON line per observation: `report(label, value)` and, for an action that may
/// throw, `attempt(label, action)` with the denial's Node-visible shape.
const PRELUDE: &str = r#"
const report = (label, value) => console.log(JSON.stringify([label, value ?? null]));
const shape = (error) => error === undefined ? 'ok' : {
    name: error.name, code: error.code ?? null, permission: error.permission ?? null,
    resource: error.resource ?? null, message: error.message, syscall: error.syscall ?? null,
    keys: Object.keys(error).filter((key) => !['stack', 'requireStack', 'url'].includes(key)),
    cause: error.cause ? shape(error.cause) : null,
};
const attempt = (label, action) => {
    let error;
    try { action(); } catch (caught) { error = caught; }
    report(label, shape(error));
};
const settle = async (label, promise) => {
    let error;
    try { await promise; } catch (caught) { error = caught; }
    report(label, shape(error));
};
"#;

/// The first `node` on `PATH` that reports `version`.
fn node(version: &str) -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|directory| directory.join("node"))
        .filter(|candidate| candidate.is_file())
        .find(|candidate| {
            Command::new(candidate)
                .arg("--version")
                .output()
                .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).trim() == version)
        })
        .unwrap_or_else(|| {
            panic!(
                "Node {version} is not on PATH; run `mise install` and test with `mise run rust`"
            )
        })
}

/// A project directory with files and directories the cases name.
struct Project {
    _directory: tempfile::TempDir,
    root: PathBuf,
}

fn project() -> Project {
    let directory = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(directory.path()).unwrap();
    std::fs::create_dir_all(root.join("existing/sub")).unwrap();
    std::fs::write(root.join("existing/sub/file.txt"), "file").unwrap();
    std::fs::write(root.join("file.txt"), "file").unwrap();
    std::fs::write(root.join("secret.txt"), "secret").unwrap();
    std::fs::write(root.join("other.mjs"), "export default 1;\n").unwrap();
    std::fs::write(root.join("package.json"), "{\"type\": \"module\"}\n").unwrap();
    for directory in ["test", "test2", "testing"] {
        std::fs::create_dir(root.join(directory)).unwrap();
    }
    Project { _directory: directory, root }
}

#[derive(Debug)]
struct Outcome {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Project {
    fn run(
        &self,
        program: &Path,
        prefix: &[&str],
        flags: &[&str],
        script: &str,
        env: &[(&str, &str)],
    ) -> Outcome {
        std::fs::write(self.root.join("probe.mjs"), format!("{PRELUDE}\n{script}")).unwrap();
        let arguments: Vec<&str> =
            prefix.iter().chain(flags).copied().chain(["probe.mjs"]).collect();
        self.command_line(program, &arguments, env)
    }

    /// Runs `program` with exactly `arguments` in the project.
    fn command_line(&self, program: &Path, arguments: &[&str], env: &[(&str, &str)]) -> Outcome {
        let output = Command::new(program)
            .args(arguments)
            .current_dir(&self.root)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &self.root)
            .envs(env.iter().copied())
            .output()
            .unwrap();
        let root = self.root.display().to_string();
        let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).replace(&root, "<root>");
        Outcome {
            code: output.status.code(),
            stdout: text(&output.stdout),
            stderr: text(&output.stderr),
        }
    }

    fn node(&self, version: &str, flags: &[&str], script: &str, env: &[(&str, &str)]) -> Outcome {
        self.run(&node(version), &[], flags, script, env)
    }

    fn bento(&self, flags: &[&str], script: &str, env: &[(&str, &str)]) -> Outcome {
        self.run(Path::new(BENTO), &["run"], flags, script, env)
    }

    /// Runs the case under both and requires the same report from each.
    fn same(
        &self,
        version: &str,
        flags: &[&str],
        script: &str,
        env: &[(&str, &str)],
    ) -> (Outcome, Outcome) {
        let node = self.node(version, flags, script, env);
        let bento = self.bento(flags, script, env);
        assert_eq!(node.code, Some(0), "Node failed: {node:?}");
        assert_eq!(bento.code, Some(0), "Bento failed: {bento:?}");
        assert_eq!(
            lines(&bento.stdout),
            lines(&node.stdout),
            "Bento's report differs from Node's\nBento stderr: {}",
            bento.stderr
        );
        (node, bento)
    }
}

fn lines(text: &str) -> Vec<&str> {
    text.lines().collect()
}

#[test]
fn file_system_flags_answer_has_as_node_does() {
    let project = project();
    project.same(
        NODE,
        &[
            "--permission",
            "--allow-fs-read=./test*",
            "--allow-fs-read=./existing",
            "--allow-fs-read",
            "./missing",
            "--allow-fs-read=./file.txt",
            "--allow-fs-write=./build/*",
            "--allow-fs-write=./q/r/*ignored",
        ],
        r#"
const { has } = process.permission;
const cwd = process.cwd();
for (const reference of ['test', 'test2/a', 'tes', 'testing/z', 'existing', 'existing/sub/file.txt', 'existingx',
    'missing', 'missing/x', 'file.txt', 'file.txt/x', 'secret.txt', 'probe.mjs']) {
    report(`read ${reference}`, has('fs.read', reference));
    report(`read absolute ${reference}`, has('fs.read', `${cwd}/${reference}`));
}
for (const reference of ['build', 'build/', 'build/x', 'buildx', 'q', 'q/r', 'q/r/s', 'q/rs']) {
    report(`write ${reference}`, has('fs.write', reference));
}
report('read url', has('fs.read', new URL(`file://${cwd}/existing/sub`)));
report('read bytes', has('fs.read', new TextEncoder().encode(`${cwd}/existing`)));
report('read buffer', has('fs.read', Buffer.from(`${cwd}/file.txt`)));
for (const scope of ['fs', 'fs.read', 'fs.write', 'net', 'child', 'worker', 'addon', 'wasi', 'ffi', 'inspector', 'openssl.store', 'fs.vfs', 'bogus']) {
    report(`scope ${scope}`, has(scope));
}
report('fs both', has('fs', 'existing'));
attempt('has without scope', () => has());
attempt('has with a number', () => has('fs.read', 5));
attempt('has with an object', () => has(Symbol('s')));
report('descriptor', Object.getOwnPropertyDescriptor(process, 'permission').enumerable);
report('keys', Object.keys(process.permission));
"#,
        &[],
    );
}

#[test]
fn wasi_and_the_inspector_are_refused_as_node_refuses_them() {
    let project = project();
    let script = r#"
import { WASI } from 'node:wasi';
import wasi from 'node:wasi';
import inspector, { Session, open, url } from 'node:inspector';
import { Session as PromiseSession } from 'node:inspector/promises';
attempt('wasi', () => new WASI({ version: 'preview1' }));
attempt('wasi default', () => new wasi.WASI({ version: 'preview1' }));
attempt('connect', () => new Session().connect());
attempt('promises connect', () => new PromiseSession().connect());
attempt('open', () => open(0));
attempt('default open', () => inspector.open(0));
report('url', url());
"#;
    project.same(NODE, &["--permission"], script, &[]);
    project.same(NODE, &["--permission", "--allow-wasi"], "import { WASI } from 'node:wasi';\nreport('wasi', typeof new WASI({ version: 'preview1' }));", &[]);
}

#[test]
fn every_boolean_flag_allows_its_scope_as_node_does() {
    let project = project();
    for (flag, scope) in [
        ("--allow-net", "net"),
        ("--allow-child-process", "child"),
        ("--allow-worker", "worker"),
        ("--allow-addons", "addon"),
        ("--allow-wasi", "wasi"),
        ("--allow-ffi", "ffi"),
        ("--allow-inspector", "inspector"),
        ("--allow-openssl-store", "openssl.store"),
        ("--allow-fs-vfs", "fs.vfs"),
    ] {
        let script = format!(
            "for (const scope of ['net', 'child', 'worker', 'addon', 'wasi', 'ffi', 'inspector', 'openssl.store', 'fs.vfs']) report(scope, process.permission.has(scope));\nreport('flag', {scope:?});"
        );
        project.same(NODE, &["--permission", flag], &script, &[]);
    }
}

#[test]
fn denials_have_nodes_code_permission_resource_and_message() {
    let project = project();
    project.same(
        NODE,
        &["--permission", "--allow-fs-read=./existing"],
        r#"
import fs from 'node:fs';
import { spawn, spawnSync, exec, execSync, execFile, execFileSync } from 'node:child_process';
import { Worker } from 'node:worker_threads';
import net from 'node:net';
import http from 'node:http';
attempt('read relative', () => fs.readFileSync('secret.txt'));
attempt('read absolute', () => fs.readFileSync(`${process.cwd()}/secret.txt`));
attempt('stat', () => fs.statSync('secret.txt'));
attempt('readdir', () => fs.readdirSync('test'));
attempt('write', () => fs.writeFileSync('existing/new.txt', 'x'));
attempt('mkdir', () => fs.mkdirSync('newdir'));
attempt('read allowed', () => fs.readFileSync('existing/sub/file.txt'));
await settle('promises read', fs.promises.readFile('secret.txt'));
await settle('callback read', new Promise((resolve, reject) => fs.readFile('secret.txt', (error) => error ? reject(error) : resolve())));
await settle('import', import('./other.mjs'));
attempt('spawn', () => spawn('ls', ['-l']));
attempt('spawnSync', () => spawnSync('/bin/echo', ['hi']));
attempt('exec', () => exec('echo hi'));
attempt('execSync', () => execSync('echo hi'));
attempt('execFile', () => execFile('/bin/ls'));
attempt('execFileSync', () => execFileSync('/bin/ls'));
attempt('worker', () => new Worker('1', { eval: true }));
attempt('listen', () => net.createServer().listen(0));
attempt('http listen', () => http.createServer().listen(0));
attempt('dlopen', () => process.dlopen({ exports: {} }, `${process.cwd()}/addon.node`));
await settle('fetch', fetch('http://127.0.0.1:8080/'));
await settle('fetch a host name', fetch('http://localhost:8080/'));
await settle('fetch IPv6', fetch('http://[::1]:8080/'));
const connection = net.connect(8080, '127.0.0.1');
report('connect', shape(await new Promise((resolve) => connection.once('error', resolve))));
"#,
        &[],
    );
}

#[test]
fn dropping_permissions_matches_node() {
    let project = project();
    project.same(
        NODE,
        &[
            "--permission",
            "--allow-fs-read=./existing",
            "--allow-fs-read=./existing/sub",
            "--allow-fs-read=./test*",
            "--allow-fs-write=*",
            "--allow-net",
            "--allow-child-process",
            "--allow-worker",
        ],
        r#"
const { has, drop } = process.permission;
const state = () => ['existing', 'existing/sub', 'existing/sub/file.txt', 'test2', 'testx'].map((path) => has('fs.read', path));
report('start', state());
drop('fs.read', 'existing'); report('drop existing', state());
drop('fs.read', 'test2'); report('drop a path the pattern covers', state());
drop('fs.read', 'test*'); report('drop the pattern', state());
drop('fs.read', 'never-allowed'); report('drop nothing', state());
drop('fs.write', '/x'); report('drop beneath *', [has('fs.write'), has('fs.write', '/x')]);
drop('fs.write', '*'); report('drop * by reference', [has('fs.write'), has('fs.write', '/x')]);
drop('net', 'example.com'); report('drop net by reference', has('net'));
drop('child'); report('drop child', has('child'));
drop('bogus'); report('drop unknown', true);
drop('fs'); report('drop fs', [has('fs.read', 'existing/sub'), has('fs.write', '/x')]);
attempt('drop without scope', () => drop());
"#,
        &[],
    );
}

/// A drop compares the whole value, text after the `*` included, so `existing/*` and
/// `existing/*ignored` are different allowances although they cover the same paths.
#[test]
fn dropping_a_pattern_compares_the_text_after_its_wildcard_as_node_does() {
    let project = project();
    let script = r#"
const { has, drop } = process.permission;
for (const reference of ['./existing/*ignored/.', 'existing/**', 'existing', 'existing/*']) {
    drop('fs.read', reference);
    report(`after dropping ${reference}`, has('fs.read', 'existing/sub/file.txt'));
}
"#;
    for allowance in ["--allow-fs-read=./existing/*", "--allow-fs-read=./existing/*ignored"] {
        project.same(NODE, &["--permission", allowance], script, &[]);
    }
}

#[test]
fn an_unrestricted_launch_matches_node_without_permission() {
    let project = project();
    project.same(
        NODE,
        &[],
        r#"
import os from 'node:os';
import fs from 'node:fs';
import { spawnSync } from 'node:child_process';
report('permission', typeof process.permission);
report('home', typeof os.homedir());
report('interfaces', typeof os.networkInterfaces());
report('read', fs.readFileSync('secret.txt', 'utf8'));
report('spawn', spawnSync('/bin/echo', ['hi'], { encoding: 'utf8' }).stdout);
"#,
        &[],
    );
}

#[test]
fn the_entry_file_is_readable_without_a_flag() {
    let project = project();
    project.same(
        NODE,
        &["--permission"],
        r#"
import fs from 'node:fs';
const entry = new URL(import.meta.url).pathname;
report('has entry', process.permission.has('fs.read', entry));
report('has all', process.permission.has('fs.read'));
report('read entry', fs.readFileSync(entry, 'utf8').length > 0);
attempt('read sibling', () => fs.readFileSync('file.txt'));
"#,
        &[],
    );
}

/// A comma is part of the path in both, and both warn about it only when the flag has exactly
/// one value, counting the script among the read values, so a script launch never warns about
/// reading.
#[test]
fn a_comma_is_part_of_the_path() {
    let project = project();
    let warned = |outcome: &Outcome, flag: &str| {
        outcome.stderr.contains(&format!("The {flag} CLI flag has changed. Passing a comma-separated list of paths is no longer valid. Documentation can be found at https://nodejs.org/api/permissions.html#file-system-permissions"))
    };
    let (node, bento) = project.same(
        NODE,
        &["--permission", "--allow-fs-read=./a,./b", "--allow-fs-write=./a,./b"],
        "report('comma path', process.permission.has('fs.read', `${process.cwd()}/a,./b`));\nreport('a', process.permission.has('fs.read', 'a'));",
        &[],
    );
    for outcome in [&node, &bento] {
        assert!(!warned(outcome, "--allow-fs-read"), "{}", outcome.stderr);
        assert!(warned(outcome, "--allow-fs-write"), "{}", outcome.stderr);
    }
}

#[test]
fn command_lines_node_refuses_bento_refuses() {
    let project = project();
    let node_path = node(NODE);
    for (flags, message) in [
        (&["--allow-net", "probe.mjs"][..], "--permission is required"),
        (&["--allow-fs-read=*", "probe.mjs"][..], "--permission is required"),
        // A flag missing its value would take a script after it as the value.
        (&["--permission", "--allow-fs-read"][..], "--allow-fs-read requires an argument"),
        (&["--permission", "--allow-fs-write"][..], "--allow-fs-write requires an argument"),
        (
            &["--permission", "--allow-fs-read=", "probe.mjs"][..],
            "--allow-fs-read= requires an argument",
        ),
    ] {
        std::fs::write(project.root.join("probe.mjs"), "console.log('ran');").unwrap();
        let node = project.command_line(&node_path, flags, &[]);
        let bento_arguments: Vec<&str> = ["run"].into_iter().chain(flags.iter().copied()).collect();
        let bento = project.command_line(Path::new(BENTO), &bento_arguments, &[]);
        assert_ne!(node.code, Some(0), "Node accepted {flags:?}");
        assert_ne!(bento.code, Some(0), "Bento accepted {flags:?}");
        assert!(node.stderr.contains(message), "Node: {}", node.stderr);
        assert!(bento.stderr.contains(message), "Bento for {flags:?}: {}", bento.stderr);
        assert!(!bento.stdout.contains("ran"), "the script ran under {flags:?}");
    }
}

#[test]
fn the_environment_matches_node_27s_allow_env() {
    let project = project();
    let env = [
        ("APP_A", "a"),
        ("APP_B", "b"),
        ("OTHER", "o"),
        ("NODE_ENV", "production"),
        ("LC_ALL", "C"),
    ];
    let (node, bento) = project.same(
        NODE_ENVIRONMENT,
        &["--permission", "--allow-fs-read=*", "--allow-env=APP_*"],
        r#"
const { has, drop } = process.permission;
report('keys', Object.keys(process.env).sort());
report('other', process.env.OTHER);
report('node env', process.env.NODE_ENV);
report('other again', process.env.OTHER);
for (const name of ['APP_A', 'APP_X', 'HOME', 'PATH', 'LC_ALL', 'OTHER', 'NODE_ENV']) report(`has ${name}`, has('env', name));
report('has env', has('env'));
process.env.OTHER = 'set';
report('set at runtime', [process.env.OTHER, has('env', 'OTHER')]);
drop('env', 'APP_A');
report('dropped', [process.env.APP_A, has('env', 'APP_A'), process.env.APP_B]);
drop('env', 'HOME');
report('dropped always kept', [process.env.HOME, has('env', 'HOME')]);
attempt('drop a wildcard', () => drop('env', 'APP_*'));
drop('env');
report('dropped everything', Object.keys(process.env).sort());
report('has after dropping everything', ['HOME', 'PATH', 'APP_B'].map((name) => has('env', name)).concat(has('env')));
drop('env', 'PATH');
report('dropped anew', [has('env', 'PATH'), process.env.PATH]);
drop('env');
report('dropped everything again', [has('env', 'PATH'), process.env.PATH]);
"#,
        &env,
    );
    for name in ["OTHER", "NODE_ENV"] {
        let warning = format!(
            "Warning: The permission model removed the environment variable \"{name}\" at startup. Use --allow-env to manage permissions."
        );
        assert_eq!(node.stderr.matches(&warning).count(), 1, "Node: {}", node.stderr);
        assert_eq!(bento.stderr.matches(&warning).count(), 1, "Bento: {}", bento.stderr);
    }
}

#[test]
fn environment_values_are_parsed_as_node_27_parses_them() {
    let project = project();
    let env = [("A", "a"), ("B", "b"), ("C", "c")];
    project.same(
        NODE_ENVIRONMENT,
        &["--permission", "--allow-env=A,,B", "--allow-env=,C,"],
        "report('values', [process.env.A, process.env.B, process.env.C]);",
        &env,
    );
    let node_path = node(NODE_ENVIRONMENT);
    for value in ["A*B", "A=B", "**"] {
        let flag = format!("--allow-env={value}");
        let flags = ["--permission", flag.as_str()];
        let node = project.run(&node_path, &[], &flags, "report('ran', true);", &[]);
        let bento = project.bento(&flags, "report('ran', true);", &[]);
        let message =
            "--allow-env must be '*', a variable name, or a variable name prefix followed by '*'";
        assert!(node.stderr.contains(message), "Node: {}", node.stderr);
        assert!(bento.stderr.contains(message), "Bento: {}", bento.stderr);
        assert_ne!(bento.code, Some(0));
    }
}

// The differences the proposal lists, each against Node's behavior.

#[test]
fn bento_checks_where_a_link_leads_where_node_checks_the_link() {
    let project = project();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "outside").unwrap();
    std::os::unix::fs::symlink(outside.path(), project.root.join("existing/escape")).unwrap();
    let flags = ["--permission", "--allow-fs-read=./existing"];
    let script = "import fs from 'node:fs';\nattempt('through link', () => fs.readFileSync('existing/escape/secret.txt'));";
    let node = project.node(NODE, &flags, script, &[]);
    let bento = project.bento(&flags, script, &[]);
    assert_eq!(node.stdout.trim(), r#"["through link","ok"]"#);
    assert!(bento.stdout.contains(r#""code":"ERR_ACCESS_DENIED""#), "{bento:?}");
}

#[test]
fn bento_rejects_a_value_given_to_a_boolean_flag_where_node_ignores_it() {
    let project = project();
    let flags = ["--permission", "--allow-net=127.0.0.1:3000"];
    let script = "report('net', process.permission.has('net'));";
    let node = project.node(NODE, &flags, script, &[]);
    let bento = project.bento(&flags, script, &[]);
    assert_eq!(node.stdout.trim(), r#"["net",true]"#);
    assert_eq!(bento.code, Some(1), "{bento:?}");
    assert!(bento.stderr.contains("--allow-net"), "{}", bento.stderr);
}

#[test]
fn a_worker_keeps_the_launchs_permissions_whatever_its_exec_argv() {
    let project = project();
    let flags = ["--permission", "--allow-worker"];
    let script = r#"
import { Worker } from 'node:worker_threads';
const worker = new Worker("require('node:worker_threads').parentPort.postMessage(typeof process.permission)", { eval: true, execArgv: [] });
report('worker sees', await new Promise((resolve) => worker.once('message', resolve)));
"#;
    let node = project.node(NODE, &flags, script, &[]);
    let bento = project.bento(&flags, script, &[]);
    assert_eq!(node.stdout.trim(), r#"["worker sees","undefined"]"#, "{node:?}");
    assert_eq!(bento.stdout.trim(), r#"["worker sees","object"]"#, "{bento:?}");
}

#[test]
fn bento_prints_none_of_nodes_stability_warnings() {
    let project = project();
    let flags = ["--permission", "--allow-net", "--allow-child-process", "--allow-addons"];
    let (node, bento) = project.same(NODE, &flags, "report('ran', true);", &[]);
    assert!(node.stderr.contains("ExperimentalWarning"), "{}", node.stderr);
    assert!(node.stderr.contains("SecurityWarning"), "{}", node.stderr);
    assert!(!bento.stderr.contains("Warning"), "{}", bento.stderr);
}

#[test]
fn a_missing_flag_value_exits_with_bentos_usage_status() {
    let project = project();
    let node = project.command_line(&node(NODE), &["--permission", "--allow-fs-read"], &[]);
    let bento =
        project.command_line(Path::new(BENTO), &["run", "--permission", "--allow-fs-read"], &[]);
    assert_eq!(node.code, Some(9), "{node:?}");
    assert_eq!(bento.code, Some(1), "{bento:?}");
}

#[test]
fn drops_stay_in_the_thread_that_makes_them() {
    let project = project();
    project.same(
        NODE,
        &["--permission", "--allow-worker", "--allow-net", "--allow-child-process"],
        r#"
import { Worker } from 'node:worker_threads';
const source = `
    const { parentPort } = require('node:worker_threads');
    parentPort.on('message', (command) => {
        if (command === 'drop') process.permission.drop('child');
        parentPort.postMessage([process.permission.has('net'), process.permission.has('child')]);
    });
`;
const ask = (worker, command) => {
    worker.postMessage(command);
    return new Promise((resolve) => worker.once('message', resolve));
};
const before = new Worker(source, { eval: true });
report('running worker', await ask(before, 'report'));
process.permission.drop('net');
report('main after its drop', process.permission.has('net'));
report('running worker after the main drop', await ask(before, 'report'));
const after = new Worker(source, { eval: true });
report('worker created after the drop', await ask(after, 'report'));
report('worker drop', await ask(before, 'drop'));
report('main after the worker drop', process.permission.has('child'));
report('other worker after the worker drop', await ask(after, 'report'));
await before.terminate();
await after.terminate();
"#,
        &[],
    );
}

#[test]
fn a_worker_and_its_processes_see_the_workers_own_environment() {
    let project = project();
    project.same(
        NODE_ENVIRONMENT,
        &[
            "--permission",
            "--allow-fs-read=*",
            "--allow-worker",
            "--allow-child-process",
            "--allow-env=SHARED",
        ],
        r#"
import { Worker } from 'node:worker_threads';
const worker = new Worker(`
    const { parentPort } = require('node:worker_threads');
    const { execFileSync } = require('node:child_process');
    parentPort.once('message', () => {
        process.permission.drop('env', 'SHARED');
        process.env.FROM_WORKER = 'w';
        const shown = execFileSync('/bin/sh', ['-c', 'printf "%s|%s|%s" "\${SHARED-unset}" "\${FROM_WORKER-unset}" "\${FROM_MAIN-unset}"'], { encoding: 'utf8' });
        parentPort.postMessage([process.env.SHARED ?? null, shown]);
    });
`, { eval: true });
process.env.FROM_MAIN = 'm';
worker.postMessage('go');
report('worker', await new Promise((resolve) => worker.once('message', resolve)));
report('main', [process.env.SHARED ?? null, process.env.FROM_WORKER ?? null]);
await worker.terminate();
"#,
        &[("SHARED", "s")],
    );
}

/// A worker may read everything beneath the working directory, as in Node, even when the thread
/// that starts it may not. A worker script outside it fails inside the worker and reaches the
/// `Worker`'s `'error'` event. Bento's worker errors carry the name and message but not yet
/// Node's own properties, so the case compares those two.
#[test]
fn a_worker_reads_beneath_the_working_directory_and_reports_other_denials_as_error_events() {
    let project = project();
    let outside = tempfile::tempdir().unwrap();
    let outside_script = outside.path().canonicalize().unwrap().join("worker.mjs");
    std::fs::write(&outside_script, "console.log('outside worker ran');").unwrap();
    project.file(
        "worker.mjs",
        "import fs from 'node:fs';\nimport { parentPort } from 'node:worker_threads';\nparentPort.postMessage([process.permission.has('fs.read', 'file.txt'), fs.readFileSync('file.txt', 'utf8').length > 0, process.permission.has('fs.read')]);",
    );
    let script = r#"
import { Worker } from 'node:worker_threads';
report('main may read beneath the working directory', process.permission.has('fs.read', 'file.txt'));
const reading = new Worker('./worker.mjs');
report('worker reads beneath it', await new Promise((resolve) => reading.once('message', resolve)));
await reading.terminate();
let failing;
attempt('construct', () => { failing = new Worker(OUTSIDE); });
const [error, code] = await Promise.all([
    new Promise((resolve) => failing.once('error', resolve)),
    new Promise((resolve) => failing.once('exit', resolve)),
]);
report('error event', [error.name, error.message]);
report('exit', code);
"#
    .replace("OUTSIDE", &format!("{:?}", outside_script.display().to_string()));
    project.same(NODE, &["--permission", "--allow-worker"], &script, &[]);
}

/// A worker created with `SHARE_ENV` does not own the environment it reads, so, as in Node 27,
/// it may not drop variables from it.
#[test]
fn a_worker_sharing_the_environment_may_not_drop_variables() {
    let project = project();
    project.same(
        NODE_ENVIRONMENT,
        &["--permission", "--allow-worker", "--allow-env=A"],
        r#"
import { Worker, SHARE_ENV } from 'node:worker_threads';
const worker = new Worker(`
    const { parentPort } = require('node:worker_threads');
    const attempt = (action) => { try { action(); return 'dropped'; } catch (error) { return [error.name, error.code, error.message]; } };
    parentPort.postMessage([
        attempt(() => process.permission.drop('env', 'A')),
        attempt(() => process.permission.drop('env')),
        process.env.A ?? null,
        process.permission.has('env', 'A'),
    ]);
`, { eval: true, env: SHARE_ENV });
report('worker', await new Promise((resolve) => worker.once('message', resolve)));
report('main', [process.env.A ?? null, process.permission.has('env', 'A')]);
await worker.terminate();
"#,
        &[("A", "a")],
    );
}

/// Node looks for a worker's script before reading it, so a missing script outside what the
/// instance may read fails as missing, not as denied. Bento's not-found message is its own, so
/// the case compares the kind of failure.
#[test]
fn a_missing_worker_script_fails_as_missing_rather_than_denied() {
    let project = project();
    let outside = tempfile::tempdir().unwrap();
    let missing = outside.path().canonicalize().unwrap().join("missing.mjs");
    let script = r#"
import { Worker } from 'node:worker_threads';
const worker = new Worker(MISSING);
const error = await new Promise((resolve) => worker.once('error', resolve));
report('failure', [error.name, error.code === 'ERR_ACCESS_DENIED', error.message.includes('restricted')]);
"#
    .replace("MISSING", &format!("{:?}", missing.display().to_string()));
    project.same(NODE, &["--permission", "--allow-worker"], &script, &[]);
}

impl Project {
    /// Writes `text` to `name` in the project, creating its directories.
    fn file(&self, name: &str, text: &str) {
        let path = self.root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

#[test]
fn a_denied_module_anywhere_in_a_dynamic_import_rejects_it_as_node_does() {
    let project = project();
    project.file("dependency.mjs", "export const item = 1;\n");
    project.file(
        "wrapper.mjs",
        "globalThis.evaluated = true;\nexport { item } from './dependency.mjs';\n",
    );
    project.file("outer.mjs", "export { item } from './wrapper.mjs';\n");
    let script = r#"
await settle('a readable module whose import is denied', import('./wrapper.mjs'));
await settle('the same module again', import('./wrapper.mjs'));
await settle('a module two imports above the denial', import('./outer.mjs'));
report('the readable module evaluated', globalThis.evaluated ?? false);
await settle('the denied module itself', import('./dependency.mjs'));
"#;
    project.same(
        NODE,
        &["--permission", "--allow-fs-read=./wrapper.mjs", "--allow-fs-read=./outer.mjs"],
        script,
        &[],
    );
    project.same(
        NODE,
        &["--permission", "--allow-fs-read=./wrapper.mjs", "--allow-fs-read=./dependency.mjs"],
        "report('item', (await import('./wrapper.mjs')).item);",
        &[],
    );
}

/// Node's resolver reads a package's `package.json` and settles on a file it then reads, so it
/// refuses either when the instance may not read it, before either can reveal anything.
#[test]
fn resolution_refuses_files_the_instance_may_not_read_as_node_does() {
    let project = project();
    project.file("node_modules/pkg/package.json", r#"{"name":"pkg","exports":"./token.js"}"#);
    project.file("node_modules/pkg/token.js", "export const token = 1;\n");
    project.file("secret.cjs", "module.exports = 1;\n");
    project.file("readable.mjs", "export const readable = 1;\n");
    project.file("src/entry.mjs", "export { inner } from './inner.mjs';\n");
    project.file("src/inner.mjs", "export const inner = 1;\n");
    let script = r#"
for (const specifier of ['pkg', './secret.cjs', './other.mjs', './readable.mjs', './src/entry.mjs']) {
    let resolved;
    attempt(`resolve ${specifier}`, () => { resolved = import.meta.resolve(specifier); });
    report(`resolved ${specifier}`, resolved);
    await settle(`import ${specifier}`, import(specifier));
}
"#;
    for flags in [
        &["--permission", "--allow-fs-read=./readable.mjs"][..],
        &["--permission", "--allow-fs-read=./node_modules/pkg/package.json"],
        &["--permission", "--allow-fs-read=./src/*"],
    ] {
        project.same(NODE, flags, script, &[]);
    }
    project.same(
        NODE,
        &["--permission"],
        r#"
attempt('resolve a subpath of a package', () => import.meta.resolve('pkg/sub'));
await settle('import a subpath of a package', import('pkg/sub'));
"#,
        &[],
    );
}

/// `require()` probes the directories of a lookup without the permission model, as Node's
/// `internalModuleStat` does, and is refused the file it settles on.
#[test]
fn require_refuses_the_file_it_resolves_as_node_does() {
    let project = project();
    project.file("secret.cjs", "module.exports = 1;\n");
    project.file("sub/x.cjs", "module.exports = 'x';\n");
    project.file("node_modules/pkg/package.json", r#"{"name":"pkg","main":"index.js"}"#);
    project.file("node_modules/pkg/index.js", "module.exports = 1;\n");
    project.same(
        NODE,
        &["--permission", "--allow-fs-read=./sub/x.cjs"],
        r#"
import Module, { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
attempt('require', () => require('./secret.cjs'));
attempt('require.resolve', () => require.resolve('./secret.cjs'));
attempt('require.resolve an absolute path', () => require.resolve(`${process.cwd()}/secret.cjs`));
attempt('require.resolve a directory', () => require.resolve('./sub'));
attempt('require a package', () => require('pkg'));
attempt('require.resolve a package', () => require.resolve('pkg'));
report('require a readable file', require('./sub/x.cjs'));
report('require.resolve a readable file', require.resolve('./sub/x.cjs'));
for (const path of ['secret.cjs', 'sub', 'absent', 'secret.cjs/x']) {
    report(`stat ${path}`, Module._stat(`${process.cwd()}/${path}`));
}
"#,
        &[],
    );
}

/// For a bare request, Node's `require()` reads `<node_modules>/<name>/package.json` before the
/// manifest's `exports` can shape the result, and is refused it even when the package is absent.
#[test]
fn require_refuses_a_package_manifest_as_node_does() {
    let project = project();
    project.file("node_modules/pkg/package.json", r#"{"name":"pkg","exports":"./token.js"}"#);
    project.file("node_modules/pkg/token.js", "module.exports = 1;\n");
    let script = r#"
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
attempt('require a package', () => require('pkg'));
attempt('require.resolve a package', () => require.resolve('pkg'));
attempt('require an absent package', () => require('absent'));
attempt('require a subpath of an absent scoped package', () => require('@scope/absent/sub'));
report('require a built-in module', typeof require('node:fs'));
"#;
    for flags in [
        &["--permission"][..],
        &["--permission", "--allow-fs-read=./node_modules/pkg/package.json"],
        &["--permission", "--allow-fs-read=./node_modules/*"],
    ] {
        project.same(NODE, flags, script, &[]);
    }
    project.same(
        NODE,
        &["--permission"],
        "import { createRequire } from 'node:module';\nattempt('require a subpath of a package', () => createRequire(import.meta.url)('pkg/sub'));",
        &[],
    );
}

/// A module that answers `_start` with nothing, for WASI to run.
const EMPTY_WASI_MODULE: &str = "[0,97,115,109,1,0,0,0,1,4,1,96,0,0,3,2,1,0,5,3,1,0,1,7,19,2,6,109,101,109,111,114,121,2,0,6,95,115,116,97,114,116,0,0,10,4,1,2,0,11]";

#[test]
fn no_path_to_the_wasi_class_escapes_its_permission() {
    let project = project();
    let script = format!(
        r#"
import {{ WASI }} from 'node:wasi';
const bytes = new Uint8Array({EMPTY_WASI_MODULE});
const options = {{ version: 'preview1', returnOnExit: true }};
const run = async (label, make) => {{
    let wasi;
    try {{ wasi = make(); }} catch (error) {{ report(label, shape(error)); return; }}
    const {{ instance }} = await WebAssembly.instantiate(bytes, wasi.getImportObject());
    report(label, [wasi instanceof WASI, wasi.start(instance)]);
}};
await run('the class', () => new WASI(options));
await run('the class it extends', () => new (Object.getPrototypeOf(WASI))(options));
await run('its prototype constructor', () => new WASI.prototype.constructor(options));
await run('a subclass', () => new (class extends WASI {{}})(options));
attempt('a call without new', () => WASI(options));
// Node validates the options before its binding checks the permission.
for (const [label, value] of [['no options', undefined], ['a bad version', {{ version: 'bogus' }}],
    ['bad args', {{ version: 'preview1', args: 5 }}], ['a bad stdin', {{ version: 'preview1', stdin: -1 }}]]) {{
    try {{ new WASI(value); report(label, 'ok'); }} catch (error) {{ report(label, [error.name, error.code, error.message]); }}
}}
"#
    );
    project.same(NODE, &["--permission"], &script, &[]);
    project.same(NODE, &["--permission", "--allow-wasi"], &script, &[]);
}

#[test]
fn an_inspector_session_in_a_worker_is_refused_as_node_refuses_it() {
    let project = project();
    let script = r#"
import { Worker } from 'node:worker_threads';
import { Session } from 'node:inspector';
const worker = new Worker(`
    const { parentPort } = require('node:worker_threads');
    const inspector = require('node:inspector');
    const { Session } = require('node:inspector/promises');
    const results = [];
    for (const [label, connect] of [
        ['connectToMainThread', () => new inspector.Session().connectToMainThread()],
        ['promises connectToMainThread', () => new Session().connectToMainThread()],
        ['connect', () => new inspector.Session().connect()],
    ]) {
        try { connect(); results.push([label, 'ok']); }
        catch (error) { results.push([label, [error.code, error.permission, error.resource, error.message]]); }
    }
    parentPort.postMessage(results);
`, { eval: true });
for (const [label, outcome] of await new Promise((resolve) => worker.once('message', resolve))) report(label, outcome);
try { new Session().connectToMainThread(); } catch (error) { report('the main thread', [error.code, error.message]); }
"#;
    project.same(NODE, &["--permission", "--allow-worker"], script, &[]);
}

#[test]
fn dns_denials_reach_callbacks_and_promises_as_in_node() {
    let project = project();
    project.same(
        NODE,
        &["--permission"],
        r#"
import dns from 'node:dns';
const dnsShape = (error) => error ? { ...shape(error), errno: error.errno ?? null, hostname: error.hostname ?? null } : 'ok';
const order = [];
const settled = [];
const track = (label) => (error) => { order.push(label); settled.push([label, dnsShape(error)]); };
const request = dns.lookup('localhost', track('lookup'));
order.push(`lookup returned ${request.constructor.name}`);
dns.lookup('localhost', { all: true }, track('lookup all'));
dns.lookupService('127.0.0.1', 22, track('lookupService'));
order.push('lookupService returned');
dns.resolve4('localhost', track('resolve4'));
dns.resolve('localhost', 'MX', track('resolve MX'));
dns.reverse('127.0.0.1', track('reverse'));
new dns.Resolver().resolveTxt('localhost', track('Resolver resolveTxt'));
process.nextTick(() => order.push('nextTick'));
setImmediate(() => order.push('setImmediate'));
await new Promise((resolve) => setTimeout(resolve, 50));
for (const [label, outcome] of settled) report(label, outcome);
report('order', order);
report('lookup an address', await new Promise((resolve) => dns.lookup('127.0.0.1', (error, address) => resolve(error ? dnsShape(error) : address))));
for (const method of ['lookup', 'lookupService', 'resolve', 'resolve4', 'resolve6', 'resolveAny', 'resolveCaa', 'resolveCname', 'resolveMx',
    'resolveNaptr', 'resolveNs', 'resolvePtr', 'resolveSoa', 'resolveSrv', 'resolveTxt', 'reverse']) {
    const host = method === 'reverse' || method === 'lookupService' ? '127.0.0.1' : 'localhost';
    let error;
    try { await (method === 'lookupService' ? dns.promises.lookupService(host, 22) : dns.promises[method](host)); } catch (caught) { error = caught; }
    report(`promises ${method}`, dnsShape(error));
}
"#,
        &[],
    );
}

/// The `errno` of Node's socket error is the denial itself.
const SOCKET_SHAPE: &str = r#"
const socketShape = (error) => ({ ...shape(error), errno: typeof error.errno === 'object' ? shape(error.errno) : error.errno ?? null,
    address: error.address ?? null, port: error.port ?? null });
"#;

#[test]
fn a_refused_udp_bind_reaches_an_error_event_as_in_node() {
    let project = project();
    let script = format!(
        r#"{SOCKET_SHAPE}
import dgram from 'node:dgram';
const outcome = (label, type, act) => new Promise((resolve) => {{
    const socket = dgram.createSocket(type);
    socket.once('error', (error) => {{ report(label, socketShape(error)); resolve(); }});
    try {{ act(socket); }} catch (error) {{ report(`${{label}} threw`, shape(error)); resolve(); }}
}});
await outcome('bind', 'udp4', (socket) => socket.bind());
await outcome('bind a port and address', 'udp4', (socket) => socket.bind(5555, '127.0.0.1'));
await outcome('bind over IPv6', 'udp6', (socket) => socket.bind(5556));
await outcome('bind a host name', 'udp4', (socket) => socket.bind(5557, 'localhost'));
await outcome('send', 'udp4', (socket) => socket.send('x', 9999, '127.0.0.1'));
await outcome('connect', 'udp4', (socket) => socket.connect(9999, '127.0.0.1'));
"#
    );
    project.same(NODE, &["--permission"], &script, &[]);
}

#[test]
fn unix_domain_sockets_need_only_the_network_permission_as_in_node() {
    let project = project();
    let script = format!(
        r#"{SOCKET_SHAPE}
import net from 'node:net';
const connected = (socket) => new Promise((resolve) => {{
    socket.once('error', (error) => resolve(socketShape(error)));
    socket.once('connect', () => {{ socket.destroy(); resolve('connected'); }});
}});
const server = net.createServer();
try {{
    await new Promise((resolve, reject) => {{ server.once('error', reject); server.listen('server.sock', resolve); }});
    report('listen', 'listening');
}} catch (error) {{ report('listen', shape(error)); }}
attempt('listen with options', () => net.createServer().listen({{ path: 'other.sock' }}).close());
report('connect', await connected(net.connect('server.sock')));
report('createConnection', await connected(net.createConnection({{ path: 'server.sock' }})));
report('connect to a missing socket', await connected(net.connect('missing.sock')));
server.close();
"#
    );
    for flags in [
        &["--permission"][..],
        &["--permission", "--allow-fs-read=*", "--allow-fs-write=*"],
        &["--permission", "--allow-net"],
    ] {
        project.same(NODE, flags, &script, &[]);
    }
}

/// Node's file system functions differ in how they report a denial: `lstat`, and the `rm` and
/// `cp` built on it, check in JavaScript with a shorter message; some callback functions throw;
/// `realpath` walks the path; and a few name the resolved path or `mkdtemp`'s template.
#[test]
fn file_system_denials_match_node_function_by_function() {
    let project = project();
    project.same(
        NODE,
        &["--permission", "--allow-fs-read=./existing"],
        r#"
import fs from 'node:fs';
import fsp from 'node:fs/promises';
import { lstatSync, watch } from 'node:fs';
import { promisify } from 'node:util';
const secret = 'secret.txt';
const readable = 'existing/sub/file.txt';
const secretUrl = new URL(`file://${process.cwd()}/secret.txt`);
for (const [label, action] of Object.entries({
    lstatSync: () => fs.lstatSync(secret),
    'lstatSync imported by name': () => lstatSync(secret),
    statSync: () => fs.statSync(secret),
    rmSync: () => fs.rmSync(secret),
    'rmSync forced': () => fs.rmSync('missing.txt', { force: true }),
    realpathSync: () => fs.realpathSync(secret),
    'realpathSync through ..': () => fs.realpathSync('existing/../secret.txt'),
    'realpathSync.native': () => fs.realpathSync.native(secret),
    mkdtempSync: () => fs.mkdtempSync('tmp-'),
    cpSync: () => fs.cpSync(readable, 'out.txt'),
    'cpSync a secret': () => fs.cpSync(secret, 'out.txt'),
    existsSync: () => fs.existsSync(secret),
    readlinkSync: () => fs.readlinkSync(secret),
    watch: () => fs.watch(secret).close(),
    'watch imported by name': () => watch(secret).close(),
    'watch a URL': () => fs.watch(secretUrl).close(),
    'watch a readable file': () => fs.watch(readable).close(),
    watchFile: () => fs.watchFile(secret, () => {}),
    'watchFile through ..': () => fs.watchFile('existing/../secret.txt', () => {}),
    'watchFile a readable file': () => { fs.watchFile(readable, () => {}); fs.unwatchFile(readable); },
})) attempt(label, action);
try { fs.watchFile(secret); } catch (error) { report('watchFile without a listener', [error.code, error.message]); }
const call = (label, invoke) => new Promise((resolve) => {
    try { invoke((error) => { report(label, shape(error ?? undefined)); resolve(); }); }
    catch (error) { report(`${label} threw`, shape(error)); resolve(); }
});
for (const [label, invoke] of Object.entries({
    lstat: (done) => fs.lstat(secret, done),
    stat: (done) => fs.stat(secret, done),
    rm: (done) => fs.rm(secret, done),
    'rm forced': (done) => fs.rm('missing.txt', { force: true }, done),
    cp: (done) => fs.cp(readable, 'out.txt', done),
    'cp dereferencing': (done) => fs.cp(secret, 'out.txt', { dereference: true }, done),
    realpath: (done) => fs.realpath(secret, done),
    'realpath of a readable file': (done) => fs.realpath(readable, done),
    'realpath.native': (done) => fs.realpath.native(secret, done),
    mkdtemp: (done) => fs.mkdtemp('tmp-', done),
    mkdir: (done) => fs.mkdir('newdir', done),
    'mkdir recursive': (done) => fs.mkdir('newdir/a', { recursive: true }, done),
    chmod: (done) => fs.chmod(secret, 0o644, done),
    utimes: (done) => fs.utimes(secret, 1, 1, done),
    lutimes: (done) => fs.lutimes(secret, 1, 1, done),
    rmdir: (done) => fs.rmdir('test', done),
    readlink: (done) => fs.readlink(secret, done),
    readFile: (done) => fs.readFile(secret, done),
    writeFile: (done) => fs.writeFile(secret, 'x', done),
    open: (done) => fs.open(secret, done),
    readdir: (done) => fs.readdir('test', done),
    access: (done) => fs.access(secret, done),
    opendir: (done) => fs.opendir('test', done),
    rename: (done) => fs.rename(secret, 'renamed.txt', done),
    copyFile: (done) => fs.copyFile(secret, 'copy.txt', done),
    symlink: (done) => fs.symlink(secret, 'link.txt', done),
    unlink: (done) => fs.unlink(secret, done),
})) await call(label, invoke);
report('exists', await new Promise((resolve) => fs.exists(secret, resolve)));
report('exists a readable file', await new Promise((resolve) => fs.exists(readable, resolve)));
report('exists promisified', await promisify(fs.exists)(secret));
for (const [label, start] of Object.entries({
    lstat: () => fsp.lstat(secret),
    'fs.promises lstat': () => fs.promises.lstat(secret),
    stat: () => fsp.stat(secret),
    rm: () => fsp.rm(secret),
    cp: () => fsp.cp(readable, 'out.txt'),
    'cp dereferencing': () => fsp.cp(secret, 'out.txt', { dereference: true }),
    realpath: () => fsp.realpath(secret),
    mkdtemp: () => fsp.mkdtemp('tmp-'),
    mkdir: () => fsp.mkdir('newdir'),
    chmod: () => fsp.chmod(secret, 0o644),
    readlink: () => fsp.readlink(secret),
    readFile: () => fsp.readFile(secret),
    access: () => fsp.access(secret),
    opendir: () => fsp.opendir('test'),
})) await settle(`promises ${label}`, start());
for (const [label, open] of [['read stream', () => fs.createReadStream(secret)], ['write stream', () => fs.createWriteStream(secret)]]) {
    report(label, await new Promise((resolve) => open().once('error', (error) => resolve(shape(error))).once('open', () => resolve('opened'))));
}
"#,
        &[],
    );
}

/// Node checks once it has validated the arguments and chosen the file, the shell for `shell`.
#[test]
fn child_process_denials_follow_nodes_validation_and_name_its_file() {
    let project = project();
    project.same(
        NODE,
        &["--permission"],
        r#"
import childProcess from 'node:child_process';
import { spawn } from 'node:child_process';
const outcome = (label, action) => {
    try { action(); report(label, 'ok'); }
    catch (error) {
        const { code, permission, resource, message } = error;
        report(label, [code, permission ?? null, resource === process.execPath ? 'execPath' : resource ?? null, message]);
    }
};
for (const [label, action] of Object.entries({
    'spawn()': () => childProcess.spawn(),
    'spawn imported by name': () => spawn(),
    'spawn(5)': () => childProcess.spawn(5),
    "spawn('')": () => childProcess.spawn(''),
    'spawnSync()': () => childProcess.spawnSync(),
    'execFile()': () => childProcess.execFile(),
    'execFileSync()': () => childProcess.execFileSync(),
    'spawn with a null byte': () => childProcess.spawn('ls\0'),
    'spawn with a bad cwd': () => childProcess.spawn('ls', { cwd: 5 }),
    'spawn with a bad shell': () => childProcess.spawn('ls', { shell: 5 }),
    'ChildProcess spawn without options': () => new childProcess.ChildProcess().spawn(),
    'ChildProcess spawn without a file': () => new childProcess.ChildProcess().spawn({}),
    spawn: () => childProcess.spawn('ls', ['-l']),
    'spawn with shell': () => childProcess.spawn('printf ok', { shell: true }),
    'spawn imported by name with shell': () => spawn('printf ok', { shell: true }),
    'spawn with a shell': () => childProcess.spawn('printf ok', { shell: '/bin/bash' }),
    'spawnSync with shell': () => childProcess.spawnSync('printf ok', { shell: true }),
    'execFile with shell': () => childProcess.execFile('echo', ['x'], { shell: true }),
    'execFileSync with a shell': () => childProcess.execFileSync('echo', ['x'], { shell: '/bin/bash' }),
    exec: () => childProcess.exec('echo hi'),
    'exec with a shell': () => childProcess.exec('echo hi', { shell: '/bin/bash' }),
    'execSync with a shell': () => childProcess.execSync('echo hi', { shell: '/bin/bash' }),
    fork: () => childProcess.fork('child.js'),
    'fork with execPath': () => childProcess.fork('child.js', [], { execPath: '/bin/echo' }),
    'ChildProcess spawn': () => new childProcess.ChildProcess().spawn({ file: 'ls', args: ['ls'] }),
})) outcome(label, action);
"#,
        &[],
    );
}

/// An empty reference names nothing: `has()` answers no, Node 26's `drop()` reads it as no
/// reference at all, and Node 27 refuses one for the environment.
#[test]
fn an_empty_reference_matches_node() {
    let project = project();
    project.same(
        NODE,
        &[
            "--permission",
            "--allow-fs-read=./existing",
            "--allow-fs-write=*",
            "--allow-net",
            "--allow-child-process",
        ],
        r#"
const { has, drop } = process.permission;
for (const scope of ['fs', 'fs.read', 'fs.write', 'net', 'child', 'worker', 'bogus']) {
    report(`has ${scope}`, [has(scope, ''), has(scope, new Uint8Array())]);
}
const state = () => [has('fs.read', 'existing'), has('fs.write', '/x'), has('net'), has('child')];
drop('fs.read', ''); report('drop fs.read', state());
drop('fs.write', new Uint8Array()); report('drop fs.write', state());
drop('net', ''); report('drop net', state());
drop('child', ''); report('drop child', state());
"#,
        &[],
    );
    project.same(
        NODE_ENVIRONMENT,
        &["--permission", "--allow-env=APP_*"],
        r#"
const { has, drop } = process.permission;
report('has env', [has('env', ''), has('env', new Uint8Array())]);
attempt('drop an empty name', () => drop('env', ''));
attempt('drop empty bytes', () => drop('env', new Uint8Array()));
report('kept', [process.env.APP_A, has('env', 'APP_A')]);
"#,
        &[("APP_A", "a")],
    );
}

/// Node's glob skips what it may not inspect, so a walk from an unreadable working directory
/// finds nothing. A walk that starts in a readable directory and climbs out of it is left out:
/// deno_node's glob rethrows the `lstat` failure there, a file system difference of its own.
#[test]
fn glob_skips_what_it_may_not_read_as_node_does() {
    let project = project();
    project.same(
        NODE,
        &["--permission", "--allow-fs-read=./existing"],
        r#"
import fs from 'node:fs';
import fsp from 'node:fs/promises';
for (const [label, pattern, options] of [['in the working directory', '*.txt', {}], ['into a readable directory', 'existing/**', {}],
    ['into unreadable directories', 'test*/*', {}]]) {
    report(`glob ${label}`, await new Promise((resolve) => fs.glob(pattern, options, (error, matches) => resolve(error ? shape(error) : matches.sort()))));
    try {
        const matches = [];
        for await (const match of fsp.glob(pattern, options)) matches.push(match);
        report(`promises glob ${label}`, matches.sort());
    } catch (error) { report(`promises glob ${label}`, shape(error)); }
}
"#,
        &[],
    );
}
