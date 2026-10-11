//! Small guests that import `host:guestbook` directly, standing in for the fixture while they
//! exercise the module boundary. Each one asserts what it observes and fails the instance when
//! the host misbehaves.

/// Appends two entries, then checks `list()` returns them oldest first with RFC 3339 UTC
/// timestamps, and that the environment carries the port and mode but no database location.
pub const ROUND_TRIP: &str = r#"
import assert from 'node:assert/strict';
import { append, list } from 'host:guestbook';

assert.deepEqual(await list(), []);
await append({ name: 'Ada', message: 'First' });
await append({ name: 'Grace', message: 'Second' });

const entries = await list();
assert.deepEqual(
    entries.map(({ id, name, message }) => [id, name, message]),
    [[1, 'Ada', 'First'], [2, 'Grace', 'Second']],
);
for (const { createdAt } of entries) {
    assert.match(createdAt, /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/);
    assert.equal(new Date(createdAt).toISOString(), createdAt.replace('Z', '.000Z'));
}

assert.equal(process.env.PORT, process.argv[2]);
assert.equal(process.env.NODE_ENV, 'development');
assert.equal(process.env.DATABASE_URL, undefined);
assert.deepEqual(Object.keys(process.env).sort(), ['NODE_ENV', 'PORT']);
"#;

/// Expects one saved entry, a refused `append` carrying the `PermissionDenied` name, and an
/// unchanged listing afterwards.
pub const READ_ONLY: &str = r#"
import assert from 'node:assert/strict';
import { append, list } from 'host:guestbook';

const before = await list();
assert.equal(before.length, 1);
await assert.rejects(
    append({ name: 'Mallory', message: 'Dropped' }),
    error => error instanceof Error && error.name === 'PermissionDenied',
);
assert.deepEqual(await list(), before);
"#;

/// Lists the entries, then tries to read the database file named by its first argument.
pub const PROBE_STORE_FILE: &str = r#"
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { readFileSync } from 'node:fs';
import { list } from 'host:guestbook';

assert.equal((await list()).length, 1);
const database = process.argv[2];
await assert.rejects(readFile(database), /permission|NotCapable|Requires/i);
assert.throws(() => readFileSync(database), /permission|NotCapable|Requires/i);
"#;

/// Imports the module without having it registered.
pub const UNREGISTERED: &str = "import 'host:guestbook';\n";

/// An HTTP guest shaped like the fixture's embedded entrypoint: `GET /` answers the entries as
/// JSON, a form `POST /` appends one and redirects, a refused append answers 500 with the error's
/// name, and `SIGTERM` closes the server and exits.
pub const SERVER: &str = r#"
import http from 'node:http';
import { append, list } from 'host:guestbook';

const server = http.createServer((request, response) => {
    handle(request, response).catch(error => {
        response.writeHead(500, { 'content-type': 'text/plain' });
        response.end(`${error.name}: ${error.message}`);
    });
});

async function handle(request, response) {
    if (request.method === 'POST') {
        const fields = new URLSearchParams(await body(request));
        await append({ name: fields.get('name'), message: fields.get('message') });
        response.writeHead(303, { location: '/' });
        response.end();
        return;
    }
    response.writeHead(200, { 'content-type': 'application/json' });
    response.end(JSON.stringify(await list()));
}

function body(request) {
    return new Promise((resolve, reject) => {
        const chunks = [];
        request.on('data', chunk => chunks.push(chunk));
        request.on('end', () => resolve(Buffer.concat(chunks).toString()));
        request.on('error', reject);
    });
}

server.listen(Number(process.env.PORT));

function shutdown() {
    server.close(() => process.exit(0));
    server.closeAllConnections();
}
process.on('SIGINT', shutdown);
process.on('SIGTERM', shutdown);
"#;

/// A guest still busy starting: it has opened its port but is waiting on startup work, like
/// asset compilation, before it would install signal handlers and begin serving.
pub const STARTING: &str = r#"
import net from 'node:net';

net.createServer().listen(Number(process.env.PORT));
await new Promise(resolve => setTimeout(resolve, 60_000));
process.on('SIGTERM', () => process.exit(0));
"#;
