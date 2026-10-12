mod support;

use std::os::unix::fs::symlink;

use bento_host::Scope;
use bento_runtime::{ErrorKind, RunOutcome, start};
use support::{completed, config, project, run};

#[test]
fn no_follow_operations_act_on_the_link_not_its_target() {
    let root = project();
    std::fs::write(root.path().join("target"), "original").unwrap();
    for link in ["link", "unlink", "rename", "replace", "remove", "async"] {
        symlink("target", root.path().join(link)).unwrap();
    }
    completed(run(
        config(root.path()),
        "entry.mjs",
        r#"
import fs from 'node:fs';
import * as promises from 'node:fs/promises';
if (!fs.lstatSync('link').isSymbolicLink()) throw Error('lstat followed link');
if (fs.readlinkSync('link') !== 'target') throw Error('readlink changed target');
fs.lutimesSync('link', 1000, 1000);
if (fs.lstatSync('link').mtimeMs !== 1000000) throw Error('lutimes did not update link');
if (fs.statSync('target').mtimeMs === 1000000) throw Error('lutimes updated target');
fs.lchownSync('link', process.getuid(), process.getgid());
fs.unlinkSync('unlink');
fs.renameSync('rename', 'moved');
fs.renameSync('link', 'replace');
if (!fs.lstatSync('moved').isSymbolicLink() || !fs.lstatSync('replace').isSymbolicLink()) throw Error('rename followed link');
fs.rmSync('remove', { recursive: true });
if (!(await promises.lstat('async')).isSymbolicLink()) throw Error('async lstat followed link');
if (await promises.readlink('async') !== 'target') throw Error('async readlink followed link');
await promises.lutimes('async', 2000, 2000);
await promises.lchown('async', process.getuid(), process.getgid());
await promises.rename('async', 'async-moved');
await promises.unlink('async-moved');
if (fs.readFileSync('target', 'utf8') !== 'original') throw Error('target changed');
"#,
    ));
    assert_eq!(std::fs::read_to_string(root.path().join("target")).unwrap(), "original");
}

#[test]
fn outside_symlink_targets_remain_denied_but_link_operations_are_allowed() {
    let root = project();
    let outside = project();
    std::fs::write(outside.path().join("secret"), "outside").unwrap();
    symlink(outside.path().join("secret"), root.path().join("link")).unwrap();
    completed(run(
        config(root.path()),
        "entry.mjs",
        r#"
import fs from 'node:fs';
for (const operation of [() => fs.readFileSync('link'), () => fs.writeFileSync('link', 'changed'), () => fs.statSync('link'), () => fs.chmodSync('link', 0o777), () => fs.truncateSync('link')]) {
  let denied = false;
  try { operation(); } catch (error) { denied = error.name === 'NotCapable'; }
  if (!denied) throw Error('following operation accessed outside target');
}
if (!fs.lstatSync('link').isSymbolicLink()) throw Error('link not accessible');
fs.lchownSync('link', process.getuid(), process.getgid());
for (const operation of [() => fs.symlinkSync(fs.readlinkSync('link'), 'created'), () => fs.symlinkSync('secret', 'created')]) {
  let denied = false;
  try { operation(); } catch (error) { denied = error.name === 'NotCapable'; }
  if (!denied) throw Error('scoped grants allowed symlink creation');
}
fs.renameSync('link', 'moved');
fs.unlinkSync('moved');
"#,
    ));
    assert_eq!(std::fs::read_to_string(outside.path().join("secret")).unwrap(), "outside");
}

#[test]
fn parent_after_symlink_obeys_os_semantics_and_scoped_grants() {
    let root = project();
    let outside = project();
    std::fs::create_dir(outside.path().join("child")).unwrap();
    std::fs::write(outside.path().join("secret.txt"), "outside").unwrap();
    std::fs::write(root.path().join("secret.txt"), "inside").unwrap();
    symlink(outside.path().join("child"), root.path().join("link")).unwrap();
    let source = "import {readFileSync} from 'node:fs'; if (readFileSync('./link/../secret.txt','utf8') !== 'outside') throw Error('lexical path replaced OS path');";
    let outcome = run(config(root.path()), "entry.mjs", source);
    assert!(matches!(outcome, RunOutcome::Failed(error) if error.kind == ErrorKind::Permission));
    let mut settings = config(root.path());
    settings.grants.read = Scope::All;
    completed(start(settings, &root.path().join("entry.mjs")).unwrap().wait());
}

#[test]
fn no_follow_checks_preserve_trailing_directory_components() {
    let root = project();
    std::fs::create_dir(root.path().join("directory")).unwrap();
    symlink("directory", root.path().join("link")).unwrap();
    completed(run(
        config(root.path()),
        "entry.mjs",
        r#"
import fs from 'node:fs';
import * as promises from 'node:fs/promises';
for (const path of ['link/', 'link/.']) {
  if (!fs.lstatSync(path).isDirectory()) throw Error('native path suffix was removed');
}
let rejected = false;
try { fs.unlinkSync('link/'); } catch { rejected = true; }
if (!rejected || !fs.lstatSync('link').isSymbolicLink()) throw Error('unlink changed requested path');
rejected = false;
try { await promises.unlink('link/'); } catch { rejected = true; }
if (!rejected || !fs.lstatSync('link').isSymbolicLink()) throw Error('async unlink changed requested path');
for (const operation of [() => fs.unlinkSync(''), () => promises.unlink('')]) {
  rejected = false;
  try { await operation(); } catch (error) { rejected = error.code === 'ENOENT'; }
  if (!rejected) throw Error('empty path was changed to the instance directory');
}
"#,
    ));
}

#[test]
fn hard_links_and_directory_mutations_preserve_link_locations() {
    let root = project();
    std::fs::write(root.path().join("target"), "original").unwrap();
    std::fs::create_dir(root.path().join("directory")).unwrap();
    symlink("target", root.path().join("link")).unwrap();
    symlink("directory", root.path().join("directory-link")).unwrap();
    symlink("missing-target", root.path().join("dangling")).unwrap();
    completed(run(
        config(root.path()),
        "entry.mjs",
        r#"
import fs from 'node:fs';
import * as promises from 'node:fs/promises';
fs.linkSync('link', 'hard-link');
await promises.link('link', 'async-hard-link');
if (!fs.lstatSync('hard-link').isSymbolicLink() || !(await promises.lstat('async-hard-link')).isSymbolicLink()) throw Error('hard link followed source');
for (const operation of [() => fs.rmdirSync('directory-link'), () => fs.mkdirSync('dangling')]) {
  let rejected = false;
  try { operation(); } catch { rejected = true; }
  if (!rejected) throw Error('directory operation followed a link');
}
for (const operation of [() => promises.rmdir('directory-link'), () => promises.mkdir('dangling')]) {
  let rejected = false;
  try { await operation(); } catch { rejected = true; }
  if (!rejected) throw Error('async directory operation followed a link');
}
if (!fs.statSync('directory').isDirectory() || fs.existsSync('missing-target')) throw Error('directory target changed');
"#,
    ));
}

#[test]
fn no_follow_and_exclusive_open_flags_reach_the_native_operation() {
    let root = project();
    std::fs::write(root.path().join("target"), "original").unwrap();
    symlink("target", root.path().join("link")).unwrap();
    symlink("missing-target", root.path().join("dangling")).unwrap();
    completed(run(
        config(root.path()),
        "entry.mjs",
        r#"
import fs from 'node:fs';
import * as promises from 'node:fs/promises';
let rejected = false;
try { const fd = fs.openSync('link', fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW); fs.closeSync(fd); }
catch (error) { rejected = error.code === 'ELOOP'; }
if (!rejected) throw Error('O_NOFOLLOW was bypassed');
rejected = false;
try { const file = await promises.open('link', fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW); await file.close(); }
catch (error) { rejected = error.code === 'ELOOP'; }
if (!rejected) throw Error('async O_NOFOLLOW was bypassed');
for (const operation of [() => { const fd = fs.openSync('dangling', 'wx'); fs.closeSync(fd); }, () => Deno.openSync('dangling', { write: true, createNew: true }).close()]) {
  let rejected = false;
  try { operation(); } catch (error) { rejected = error.code === 'EEXIST' || error.name === 'AlreadyExists'; }
  if (!rejected) throw Error('exclusive open followed dangling link');
}
if (fs.existsSync('missing-target')) throw Error('exclusive open created link target');
"#,
    ));
}

#[test]
fn symlink_creation_requires_unscoped_read_and_write_for_every_api() {
    let root = project();
    let outside = project();
    for (read_all, write_all) in [(false, false), (true, false), (false, true)] {
        let mut settings = config(root.path());
        if read_all {
            settings.grants.read = Scope::All;
        }
        if write_all {
            settings.grants.write = Scope::All;
        }
        completed(run(
            settings,
            "entry.mjs",
            &format!(
                r#"
import fs from 'node:fs';
import * as promises from 'node:fs/promises';
const target = {};
for (const operation of [
  () => fs.symlinkSync(target, 'evil'),
  () => promises.symlink(target, 'evil'),
  () => Deno.symlinkSync(target, 'evil'),
  () => Deno.symlink(target, 'evil'),
]) {{
  let denied = false;
  try {{ await operation(); }} catch (error) {{ denied = error.name === 'NotCapable'; }}
  if (!denied) throw Error('scoped grants created an outside symlink');
}}
"#,
                serde_json::to_string(&outside.path().join("pwned")).unwrap()
            ),
        ));
        assert!(std::fs::symlink_metadata(root.path().join("evil")).is_err());
        assert!(!outside.path().join("pwned").exists());
    }
    let mut settings = config(root.path());
    settings.grants.read = Scope::All;
    settings.grants.write = Scope::All;
    completed(run(
        settings,
        "unrestricted.mjs",
        r#"
import fs from 'node:fs';
import * as promises from 'node:fs/promises';
fs.symlinkSync('target', 'created');
await promises.symlink('target', 'async-created');
if (fs.readlinkSync('created') !== 'target' || await promises.readlink('async-created') !== 'target') throw Error('symlink creation changed target');
"#,
    ));
}

#[test]
fn following_dangling_links_cannot_create_ungranted_targets() {
    let root = project();
    let outside = project();
    std::fs::write(root.path().join("source"), "inside").unwrap();
    symlink(outside.path().join("pwned"), root.path().join("evil")).unwrap();
    symlink(outside.path().join("newdir"), root.path().join("directory")).unwrap();
    completed(run(
        config(root.path()),
        "entry.mjs",
        r#"
import fs from 'node:fs';
import * as promises from 'node:fs/promises';
for (const operation of [
  () => fs.copyFileSync('source', 'evil'),
  () => promises.copyFile('source', 'evil'),
  () => fs.writeFileSync('evil', 'changed'),
  () => promises.writeFile('evil', 'changed'),
  () => fs.mkdirSync('directory/'),
  () => promises.mkdir('directory/'),
]) {
  let denied = false;
  try { await operation(); } catch (error) { denied = error.name === 'NotCapable'; }
  if (!denied) throw Error('following dangling link created outside target');
}
"#,
    ));
    assert!(!outside.path().join("pwned").exists());
    assert!(!outside.path().join("newdir").exists());
}
