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

/// A comma is part of the path in both. Bento always warns about it, as the proposal says;
/// Node 26 warns only when the flag has exactly one value, and it counts the implied entry
/// file among the values, so for a script it never warns.
#[test]
fn a_comma_is_part_of_the_path() {
    let project = project();
    let (_, bento) = project.same(
        NODE,
        &["--permission", "--allow-fs-read=./a,./b"],
        "report('comma path', process.permission.has('fs.read', `${process.cwd()}/a,./b`));\nreport('a', process.permission.has('fs.read', 'a'));",
        &[],
    );
    let warning = "warning: The --allow-fs-read CLI flag has changed. Passing a comma-separated list of paths is no longer valid. Documentation can be found at https://nodejs.org/api/permissions.html#file-system-permissions";
    assert!(bento.stderr.contains(warning), "{}", bento.stderr);
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
fn a_worker_keeps_its_creators_permissions_whatever_its_exec_argv() {
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
fn a_drop_reaches_workers_already_running_where_node_leaves_them_unchanged() {
    let project = project();
    let flags = ["--permission", "--allow-worker", "--allow-net"];
    let script = r#"
import { Worker } from 'node:worker_threads';
const worker = new Worker(`
    const { parentPort } = require('node:worker_threads');
    parentPort.on('message', () => parentPort.postMessage(process.permission.has('net')));
`, { eval: true });
process.permission.drop('net');
worker.postMessage('report');
report('worker has net', await new Promise((resolve) => worker.once('message', resolve)));
await worker.terminate();
"#;
    let node = project.node(NODE, &flags, script, &[]);
    let bento = project.bento(&flags, script, &[]);
    assert_eq!(node.stdout.trim(), r#"["worker has net",true]"#, "{node:?}");
    assert_eq!(bento.stdout.trim(), r#"["worker has net",false]"#, "{bento:?}");
}
