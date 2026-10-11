mod support;

use std::collections::BTreeSet;
use std::net::TcpListener;
use std::time::{Duration, Instant};

use bento_host::Scope;
use bento_runtime::{RunOutcome, ShutdownOutcome, start};

use support::{completed, config, fixture, run};

#[test]
fn pnpm_exports_imports_commonjs_and_real_native_assets() {
    let directory = fixture();
    let root = directory.path();
    std::fs::write(root.join("app/runtime-probe.mjs"), "export const probe = 'imports';").unwrap();
    let mut settings = config(root);
    settings.grants.read = Scope::All;
    settings.grants.addons =
        Scope::Only(BTreeSet::from([settings.project_root.join("node_modules")]));
    completed(run(
        settings,
        "native.mjs",
        r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import * as schema from 'remix/data-schema';
import { createAssetServer } from 'remix/assets';
import { loadConfig } from 'remix/cli';
import { probe } from '#app/runtime-probe.mjs';
assert.equal(probe, 'imports'); assert.ok(Object.keys(schema).length > 0);
const require = createRequire(import.meta.resolve('remix/assets'));
const transformPath = require.resolve('oxc-transform');
assert.ok(transformPath.includes('/.pnpm/'));
const { transform } = await import(transformPath);
const { parseSync, visitorKeys } = require('oxc-parser');
const { ResolverFactory } = require('oxc-resolver');
const { minify } = require('oxc-minify');
const lightningcss = require('lightningcss');
fs.writeFileSync('named-native.mjs', `import { transform } from ${JSON.stringify(require.resolve('lightningcss'))}; export { transform };`);
assert.equal((await import('./named-native.mjs')).transform, lightningcss.transform);
const tsx = 'export const view = <div>Bento</div>;';
assert.equal(parseSync('view.tsx', tsx).errors.length, 0);
assert.ok(visitorKeys.Program.includes('body'));
const resolver = new ResolverFactory({ conditionNames: ['node', 'import', 'default'] });
assert.ok(resolver.sync(process.cwd(), 'remix/data-schema').path.includes('/.pnpm/'));
const transformed = await transform('view.tsx', tsx, { jsx: { runtime: 'automatic', importSource: 'remix/component' }, sourcemap: true });
assert.ok(transformed.code.includes('remix/component/jsx-runtime')); assert.ok(transformed.map);
const minified = await minify('view.js', transformed.code, { module: true, compress: true, mangle: true });
assert.ok(minified.code.length > 0 && minified.code.length < transformed.code.length);
const cssPath = 'app/actions/public/preflight.css';
const css = lightningcss.transform({ filename: cssPath, code: fs.readFileSync(cssPath), minify: true });
assert.ok(css.code.length > 0 && css.code.length < fs.readFileSync(cssPath).length);
const configuration = await loadConfig(path.join(process.cwd(), 'app/utils'));
const developmentLengths = new Map();
for (const production of [false, true]) {
  const assets = createAssetServer({ ...configuration.assets, sourceMaps: production ? undefined : 'external', minify: production, fingerprint: production, watch: false });
  for (const file of ['app/actions/public/entry.ts', 'app/actions/guest-book/public/message-field.tsx', cssPath]) {
    const href = await assets.getHref(file);
    const response = await assets.fetch(new Request(new URL(href, 'http://localhost')));
    assert.equal(response.status, 200, `${file}: ${await response.clone().text()}`);
    const body = await response.text(); assert.ok(body.length > 0);
    if (production) {
      assert.match(href, /\.@[A-Za-z0-9_-]{6}\./);
      assert.ok(body.length < developmentLengths.get(file), file);
      assert.ok(!body.includes('sourceMappingURL'), file);
      assert.ok(response.headers.get('cache-control').includes('immutable'));
    } else {
      developmentLengths.set(file, body.length);
      assert.ok(!href.includes('.@'), href);
      const mapName = body.match(/sourceMappingURL=([^\s*]+)/)?.[1];
      assert.ok(mapName, file);
      const mapResponse = await assets.fetch(new Request(new URL(mapName, new URL(href, 'http://localhost'))));
      assert.equal(mapResponse.status, 200);
      const map = await mapResponse.json();
      assert.equal(map.version, 3); assert.ok(map.sources.length > 0);
    }
  }
  await assets.close();
}
"#,
    ));
}

#[test]
fn native_addons_and_large_heaps_shutdown_and_recreate_on_the_same_port() {
    let directory = fixture();
    let root = directory.path();
    let port = TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port();
    for cycle in 0..3 {
        let mut settings = config(root);
        settings.grants.net = Scope::All;
        settings.grants.read = Scope::All;
        settings.grants.addons =
            Scope::Only(BTreeSet::from([settings.project_root.join("node_modules")]));
        settings.shutdown_deadline = Duration::from_millis(200);
        let entry = root.join(format!("native-shutdown-{cycle}.mjs"));
        let marker = format!("native-ready-{cycle}");
        let handler = if cycle == 1 {
            "while (true) {}"
        } else {
            "server.close(() => process.exit(0)); server.closeAllConnections();"
        };
        std::fs::write(
            &entry,
            format!(
                r#"
import {{ createRequire }} from 'node:module';
import {{ writeFileSync }} from 'node:fs';
import http from 'node:http';
const require = createRequire(import.meta.resolve('remix/assets'));
const {{ parseSync }} = require('oxc-parser');
const {{ transform }} = require('oxc-transform');
const {{ minify }} = require('oxc-minify');
const {{ ResolverFactory }} = require('oxc-resolver');
const lightningcss = require('lightningcss');
parseSync('native.ts', 'const n: number = 1;');
await transform('native.ts', 'export const n: number = 1;');
await minify('native.js', 'export const n = 1;');
new ResolverFactory().sync(process.cwd(), 'remix/assets');
lightningcss.transform({{ filename: 'native.css', code: Buffer.from('body {{ color: red; }}') }});
globalThis.retainedHeap = Array.from({{ length: 2_000_000 }}, (_, index) => index);
const server = http.createServer((req, res) => res.end('native'));
process.on('SIGTERM', () => {{ {handler} }});
server.listen({port}, '127.0.0.1', () => writeFileSync('{marker}', 'ready'));
"#
            ),
        )
        .unwrap();
        let runtime = start(settings, &entry).unwrap();
        let ready_deadline = Instant::now() + Duration::from_secs(10);
        while !root.join(&marker).exists() {
            assert!(Instant::now() < ready_deadline, "native guest did not become ready");
            std::thread::sleep(Duration::from_millis(5));
        }
        let started = Instant::now();
        let expected = if cycle == 1 {
            ShutdownOutcome::Interrupted
        } else {
            ShutdownOutcome::Graceful { exit_code: Some(0) }
        };
        assert_eq!(runtime.handle().shutdown().outcome, expected);
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(matches!(runtime.wait(), RunOutcome::Shutdown(_)));
        drop(TcpListener::bind(("127.0.0.1", port)).expect("native guest retained the port"));
    }
}
