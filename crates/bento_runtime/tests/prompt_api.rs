mod support;

use bento_runtime::{ErrorKind, RunOutcome};

use support::{completed, config, model_access, project, run};

#[test]
fn language_model_access_requires_a_host_grant() {
    let root = project();
    completed(run(
        config(root.path()),
        "prompt.mjs",
        r#"
import assert from 'node:assert/strict';
assert.equal(typeof globalThis.LanguageModel, 'function', 'Prompt API must be exposed');
assert.throws(() => new LanguageModel(), TypeError);
for (const method of ['prompt', 'promptStreaming', 'append', 'measureContextUsage', 'clone', 'destroy']) {
    assert.equal(typeof LanguageModel.prototype[method], 'function', method);
}
assert.equal(await LanguageModel.availability(), 'unavailable');
await assert.rejects(LanguageModel.create(), error =>
    error instanceof DOMException && error.name === 'NotAllowedError');
"#,
    ));
}

#[test]
fn uncaught_language_model_denial_is_a_permission_failure() {
    let root = project();
    let outcome = run(config(root.path()), "denied.mjs", "await LanguageModel.create();");
    let RunOutcome::Failed(error) = outcome else {
        panic!("denied creation did not fail the run");
    };
    assert_eq!(error.kind, ErrorKind::Permission, "{error:?}");
}

#[test]
fn denied_access_validates_arguments_and_signals_first() {
    let root = project();
    completed(run(
        config(root.path()),
        "order.mjs",
        r#"
import assert from 'node:assert/strict';
await assert.rejects(
    LanguageModel.create({ expectedInputs: [{ type: 'text', languages: ['not a tag'] }] }),
    RangeError,
    'invalid language tags are rejected before the grant is consulted');
await assert.rejects(
    LanguageModel.availability({ expectedOutputs: [{ type: 'text', languages: ['nope!'] }] }),
    RangeError);

const reason = { why: 'already aborted' };
await assert.rejects(
    LanguageModel.create({ signal: AbortSignal.abort(reason) }),
    error => error === reason,
    'an aborted creation signal precedes the denial');
"#,
    ));
}

#[test]
fn language_model_validates_messages_before_reaching_a_backend() {
    let root = project();
    completed(run(
        model_access(root.path()),
        "messages.mjs",
        r#"
import assert from 'node:assert/strict';

const create = initialPrompts => LanguageModel.create({ initialPrompts });

await assert.rejects(create([
    { role: 'system', content: 'First.' },
    { role: 'system', content: 'Second.' },
]), TypeError, 'at most one system message is allowed');

await assert.rejects(create([
    { role: 'user', content: 'Hello.' },
    { role: 'system', content: 'Late instructions.' },
]), TypeError, 'a system message must come first');

await assert.rejects(create([
    { role: 'user', content: 'Partial.', prefix: true },
]), error => error instanceof DOMException && error.name === 'SyntaxError',
    'only an assistant message may be a prefix');

await assert.rejects(create([
    { role: 'assistant', content: 'Partial.', prefix: true },
    { role: 'user', content: 'More.' },
]), error => error instanceof DOMException && error.name === 'SyntaxError',
    'a prefix must be the last message');

await assert.rejects(create([
    { role: 'user', content: 'Finish this.' },
    { role: 'assistant', content: 'Once upon', prefix: true },
]), error => error instanceof DOMException && error.name === 'NotSupportedError',
    'assistant continuation is an unsupported capability, not a syntax error');

await assert.rejects(create([
    { role: 'user', content: [{ type: 'image', value: new Uint8Array(1) }] },
]), error => error instanceof DOMException && error.name === 'NotSupportedError',
    'image content is unsupported');

await assert.rejects(create([{ role: 'narrator', content: 'Hello.' }]), TypeError);
await assert.rejects(create([{ content: 'No role.' }]), TypeError);
await assert.rejects(create('not a message list'), TypeError);
"#,
    ));
}

#[test]
fn binary_and_image_values_cannot_be_relabelled_as_text() {
    let root = project();
    completed(run(
        model_access(root.path()),
        "non-text.mjs",
        r#"
import assert from 'node:assert/strict';

const values = [
    new Uint8Array([7]),
    new ArrayBuffer(1),
    new Blob(['kelp']),
    new ImageData(1, 1),
    await createImageBitmap(new ImageData(1, 1)),
    new OffscreenCanvas(1, 1),
];
for (const value of values) {
    await assert.rejects(LanguageModel.create({
        initialPrompts: [{ role: 'user', content: [{ type: 'text', value }] }],
    }), TypeError, value.constructor.name);
}
"#,
    ));
}

#[test]
fn language_model_canonicalizes_language_tags_and_capabilities() {
    let root = project();
    completed(run(
        model_access(root.path()),
        "capabilities.mjs",
        r#"
import assert from 'node:assert/strict';

assert.ok(['available', 'unavailable'].includes(
    await LanguageModel.availability({
        expectedInputs: [{ type: 'text', languages: ['EN-us', 'en-US', 'en'] }],
    })),
    'mixed-case and duplicate tags are canonicalized, not rejected');

await assert.rejects(
    LanguageModel.availability({ expectedInputs: [{ type: 'text', languages: ['en', '!'] }] }),
    RangeError);

assert.equal(
    await LanguageModel.availability({ expectedInputs: [{ type: 'image' }] }),
    'unavailable',
    'image input is a well-formed but unsupported capability');
assert.equal(
    await LanguageModel.availability({ expectedInputs: [{ type: 'audio' }] }),
    'unavailable');
assert.equal(
    await LanguageModel.availability({ expectedOutputs: [{ type: 'image' }] }),
    'unavailable');
await assert.rejects(
    LanguageModel.availability({ expectedInputs: [{ type: 'hologram' }] }),
    TypeError,
    'an invalid enum value is a WebIDL failure');
await assert.rejects(
    LanguageModel.create({ expectedInputs: [{ type: 'image' }] }),
    error => error instanceof DOMException && error.name === 'NotSupportedError',
    'creating a session for an unsupported capability fails');
"#,
    ));
}

#[test]
fn creation_monitor_receives_an_event_target_and_propagates_failure() {
    let root = project();
    completed(run(
        model_access(root.path()),
        "monitor.mjs",
        r#"
import assert from 'node:assert/strict';

let received = null;
const failure = new Error('monitor refused');
await assert.rejects(
    LanguageModel.create({ monitor(monitor) { received = monitor; throw failure; } }),
    error => error === failure,
    'a throwing monitor callback rejects creation with its own exception');
assert.ok(received instanceof EventTarget, 'the monitor is an EventTarget');
assert.equal(typeof received.ondownloadprogress, 'object');
assert.equal(received.ondownloadprogress, null);
"#,
    ));
}

#[cfg(not(feature = "prompt-foundation-models"))]
#[test]
fn a_backend_free_build_reports_unavailable_with_a_grant() {
    let root = project();
    completed(run(
        model_access(root.path()),
        "backend-free.mjs",
        r#"
import assert from 'node:assert/strict';
assert.equal(await LanguageModel.availability(), 'unavailable',
    'a build without a model backend is honestly unavailable');
await assert.rejects(LanguageModel.create(), error =>
    error instanceof DOMException && error.name === 'NotSupportedError',
    'creation fails for an unavailable model, not with a permission error');
"#,
    ));
}

#[test]
fn workers_expose_the_language_model_surface_with_the_inherited_grant() {
    let root = project();
    std::fs::write(
        root.path().join("probe.mjs"),
        r#"
export async function probe() {
    const results = { exposed: typeof globalThis.LanguageModel, availability: null, denied: null };
    results.availability = await LanguageModel.availability();
    try {
        await LanguageModel.create();
        results.denied = 'created';
    } catch (error) {
        results.denied = error.name;
    }
    return results;
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.path().join("web-worker.mjs"),
        r#"
import { probe } from './probe.mjs';
postMessage(await probe());
"#,
    )
    .unwrap();
    std::fs::write(
        root.path().join("node-worker.mjs"),
        r#"
import { parentPort } from 'node:worker_threads';
import { probe } from './probe.mjs';
parentPort.postMessage(await probe());
"#,
    )
    .unwrap();
    completed(run(
        config(root.path()),
        "worker-isolation.mjs",
        r#"
import assert from 'node:assert/strict';
import { Worker as NodeWorker } from 'node:worker_threads';
import { probe } from './probe.mjs';

const web = await new Promise((resolve, reject) => {
    const worker = new Worker(new URL('./web-worker.mjs', import.meta.url), { type: 'module' });
    worker.onmessage = event => { worker.terminate(); resolve(event.data); };
    worker.onerror = reject;
});
const node = await new Promise((resolve, reject) => {
    const worker = new NodeWorker(new URL('./node-worker.mjs', import.meta.url));
    worker.once('message', async answer => { await worker.terminate(); resolve(answer); });
    worker.once('error', reject);
});

for (const [isolate, result] of [['web worker', web], ['node worker', node]]) {
    assert.equal(result.exposed, 'function', `${isolate} exposes LanguageModel`);
    assert.equal(result.availability, 'unavailable', `${isolate} inherits the denial`);
    assert.equal(result.denied, 'NotAllowedError', `${isolate} denies creation`);
}
assert.deepEqual(await probe(), { exposed: 'function', availability: 'unavailable', denied: 'NotAllowedError' });
"#,
    ));
}
