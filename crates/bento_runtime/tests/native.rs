mod support;

use std::collections::BTreeSet;

use bento_host::Scope;

use support::{completed, config, fixture, run};

#[test]
fn pnpm_exports_imports_commonjs_and_real_native_assets() {
    let root = fixture();
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
