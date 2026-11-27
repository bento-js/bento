mod support;

use support::{completed, config, project, run};

#[test]
#[ignore = "fails until the fs native-path fix lands: invalid paths reach their lexically normalized target"]
fn following_node_operations_preserve_invalid_native_paths() {
    let root = project();
    std::fs::write(root.path().join("target"), "original").unwrap();
    completed(run(
        config(root.path()),
        "entry.mjs",
        r#"
import fs from 'node:fs';
import * as promises from 'node:fs/promises';
const callback = (operation, ...arguments_) => new Promise((resolve, reject) => {
  operation(...arguments_, (error, result) => error ? reject(error) : resolve(result));
});
const failures = [];
for (const path of ['missing/../target', '']) {
  const operations = {
    readSync: () => fs.readFileSync(path),
    readAsync: () => promises.readFile(path),
    readCallback: () => callback(fs.readFile, path),
    writeSync: () => fs.writeFileSync(path, 'changed'),
    writeAsync: () => promises.writeFile(path, 'changed'),
    writeCallback: () => callback(fs.writeFile, path, 'changed'),
    appendSync: () => fs.appendFileSync(path, 'changed'),
    appendAsync: () => promises.appendFile(path, 'changed'),
    appendCallback: () => callback(fs.appendFile, path, 'changed'),
    openSync: () => fs.closeSync(fs.openSync(path, 'r')),
    openAsync: async () => (await promises.open(path, 'r')).close(),
    openCallback: async () => fs.closeSync(await callback(fs.open, path, 'r')),
    statSync: () => fs.statSync(path),
    statAsync: () => promises.stat(path),
    statCallback: () => callback(fs.stat, path),
    readdirSync: () => fs.readdirSync(path),
    readdirAsync: () => promises.readdir(path),
    readdirCallback: () => callback(fs.readdir, path),
    realpathSyncNative: () => fs.realpathSync.native(path),
    realpathAsync: () => promises.realpath(path),
    realpathCallbackNative: () => callback(fs.realpath.native, path),
    mkdirSync: () => fs.mkdirSync(path),
    mkdirAsync: () => promises.mkdir(path),
    mkdirCallback: () => callback(fs.mkdir, path),
    copySourceSync: () => fs.copyFileSync(path, 'copy'),
    copySourceAsync: () => promises.copyFile(path, 'copy'),
    copySourceCallback: () => callback(fs.copyFile, path, 'copy'),
    copyTargetSync: () => fs.copyFileSync('target', path),
    copyTargetAsync: () => promises.copyFile('target', path),
    copyTargetCallback: () => callback(fs.copyFile, 'target', path),
    cpSourceSync: () => fs.cpSync(path, 'copy'),
    cpSourceAsync: () => promises.cp(path, 'copy'),
    cpSourceCallback: () => callback(fs.cp, path, 'copy'),
    renameTargetSync: () => fs.renameSync('target', path),
    renameTargetAsync: () => promises.rename('target', path),
    renameTargetCallback: () => callback(fs.rename, 'target', path),
    linkTargetSync: () => fs.linkSync('target', path),
    linkTargetAsync: () => promises.link('target', path),
    linkTargetCallback: () => callback(fs.link, 'target', path),
    truncateSync: () => fs.truncateSync(path),
    truncateAsync: () => promises.truncate(path),
    truncateCallback: () => callback(fs.truncate, path),
    utimesSync: () => fs.utimesSync(path, 1000, 1000),
    utimesAsync: () => promises.utimes(path, 1000, 1000),
    utimesCallback: () => callback(fs.utimes, path, 1000, 1000),
    chmodSync: () => fs.chmodSync(path, 0o600),
    chmodAsync: () => promises.chmod(path, 0o600),
    chmodCallback: () => callback(fs.chmod, path, 0o600),
    chownSync: () => fs.chownSync(path, process.getuid(), process.getgid()),
    chownAsync: () => promises.chown(path, process.getuid(), process.getgid()),
    chownCallback: () => callback(fs.chown, path, process.getuid(), process.getgid()),
    accessSync: () => fs.accessSync(path),
    accessAsync: () => promises.access(path),
    accessCallback: () => callback(fs.access, path),
    watch: () => fs.watch(path).close(),
  };
  for (const [name, operation] of Object.entries(operations)) {
    try {
      await operation();
      failures.push(`${JSON.stringify(path)} ${name}: succeeded`);
    } catch (error) {
      if (error.code !== 'ENOENT') failures.push(`${JSON.stringify(path)} ${name}: ${error.code}`);
    }
  }
  if (fs.existsSync(path) || await new Promise(resolve => fs.exists(path, resolve))) failures.push(`${JSON.stringify(path)} exists: true`);
}
if (fs.readFileSync('target', 'utf8') !== 'original') failures.push('target was changed');
if (failures.length) throw Error(failures.join('\n'));
"#,
    ));
    assert_eq!(std::fs::read_to_string(root.path().join("target")).unwrap(), "original");
}

#[test]
#[ignore = "fails until the fs native-path fix lands: invalid paths reach their lexically normalized target"]
fn following_deno_operations_preserve_invalid_native_paths() {
    let root = project();
    std::fs::write(root.path().join("target"), "original").unwrap();
    completed(run(
        config(root.path()),
        "entry.mjs",
        r#"
const failures = [];
for (const path of ['missing/../target', '']) {
  const operations = {
    readSync: () => Deno.readFileSync(path),
    readAsync: () => Deno.readFile(path),
    readTextSync: () => Deno.readTextFileSync(path),
    readTextAsync: () => Deno.readTextFile(path),
    writeSync: () => Deno.writeFileSync(path, new TextEncoder().encode('changed')),
    writeAsync: () => Deno.writeFile(path, new TextEncoder().encode('changed')),
    writeTextSync: () => Deno.writeTextFileSync(path, 'changed'),
    writeTextAsync: () => Deno.writeTextFile(path, 'changed'),
    openSync: () => Deno.openSync(path).close(),
    openAsync: async () => (await Deno.open(path)).close(),
    statSync: () => Deno.statSync(path),
    statAsync: () => Deno.stat(path),
    readdirSync: () => [...Deno.readDirSync(path)],
    readdirAsync: async () => { for await (const entry of Deno.readDir(path)) {} },
    realpathSync: () => Deno.realPathSync(path),
    realpathAsync: () => Deno.realPath(path),
    mkdirSync: () => Deno.mkdirSync(path),
    mkdirAsync: () => Deno.mkdir(path),
    copySourceSync: () => Deno.copyFileSync(path, 'copy'),
    copySourceAsync: () => Deno.copyFile(path, 'copy'),
    copyTargetSync: () => Deno.copyFileSync('target', path),
    copyTargetAsync: () => Deno.copyFile('target', path),
    renameTargetSync: () => Deno.renameSync('target', path),
    renameTargetAsync: () => Deno.rename('target', path),
    linkTargetSync: () => Deno.linkSync('target', path),
    linkTargetAsync: () => Deno.link('target', path),
    truncateSync: () => Deno.truncateSync(path),
    truncateAsync: () => Deno.truncate(path),
    utimesSync: () => Deno.utimeSync(path, 1000, 1000),
    utimesAsync: () => Deno.utime(path, 1000, 1000),
    chmodSync: () => Deno.chmodSync(path, 0o600),
    chmodAsync: () => Deno.chmod(path, 0o600),
    chownSync: () => Deno.chownSync(path, Deno.uid(), Deno.gid()),
    chownAsync: () => Deno.chown(path, Deno.uid(), Deno.gid()),
    watch: () => Deno.watchFs(path).close(),
  };
  for (const [name, operation] of Object.entries(operations)) {
    try {
      await operation();
      failures.push(`${JSON.stringify(path)} ${name}: succeeded`);
    } catch (error) {
      if (error.name !== 'NotFound') failures.push(`${JSON.stringify(path)} ${name}: ${error.name}`);
    }
  }
}
if (Deno.readTextFileSync('target') !== 'original') failures.push('target was changed');
if (failures.length) throw Error(failures.join('\n'));
"#,
    ));
    assert_eq!(std::fs::read_to_string(root.path().join("target")).unwrap(), "original");
}

#[test]
#[ignore = "fails until the fs native-path fix lands: invalid paths reach their lexically normalized target"]
fn generated_missing_parent_traversals_never_reach_existing_targets() {
    let root = project();
    completed(run(
        config(root.path()),
        "entry.mjs",
        r#"
import fs from 'node:fs';
import * as promises from 'node:fs/promises';
for (let depth = 1; depth <= 8; depth++) {
  const directory = Array.from({length: depth}, (_, index) => `existing-${index}`).join('/');
  fs.mkdirSync(directory, {recursive: true});
  fs.writeFileSync(`${directory}/target`, 'original');
  for (let missingDepth = 1; missingDepth <= 8; missingDepth++) {
    const missing = Array.from({length: missingDepth}, (_, index) => `missing-${index}`).join('/');
    const parent = Array.from({length: missingDepth}, () => '..').join('/');
    for (const prefix of ['', './']) {
      const path = `${prefix}${directory}/${missing}/${parent}/target`;
      for (const operation of [() => fs.readFileSync(path), () => promises.readFile(path),
        () => fs.writeFileSync(path, 'changed'), () => promises.writeFile(path, 'changed')]) {
        let rejected = false;
        try { await operation(); } catch (error) { rejected = error.code === 'ENOENT'; }
        if (!rejected) throw Error(`native operation reached lexically normalized path: ${path}`);
      }
      if (fs.readFileSync(`${directory}/target`, 'utf8') !== 'original') throw Error(`target changed: ${path}`);
    }
  }
}
"#,
    ));
}
