mod support;

use support::{completed, config, fixture, run};

#[test]
fn top_level_await_tsx_npm_and_enums_execute_without_emitted_javascript() {
    let directory = fixture();
    let root = directory.path();
    completed(run(
        config(root),
        "direct-source.tsx",
        r#"
import assert from 'node:assert/strict';
import type { Handle } from 'remix/component';
import { renderToString } from 'remix/component/server';
import * as schema from 'remix/data-schema';
enum Message { Text = 'Direct TypeScript rendering' }
function Greeting(handle: Handle<{ text: string }>) {
  return () => <h1>{handle.props.text}</h1>;
}
const message: string = await Promise.resolve(Message.Text);
assert.ok(Object.keys(schema).length > 0);
assert.equal(await renderToString(<Greeting text={message} />), '<h1>Direct TypeScript rendering</h1>');
console.log('direct source executed');
"#,
    ));
    assert!(!root.join("direct-source.js").exists());
    assert!(!root.join("direct-source.js.map").exists());
    let fixture_path = root.to_owned();
    drop(directory);
    assert!(!fixture_path.exists(), "disposable fixture was retained");
}
