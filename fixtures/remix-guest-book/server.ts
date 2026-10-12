import * as http from "node:http";
import { createRequestListener } from "remix/node-fetch-server";

import { createDataTableStore } from "#app/data/data-table-store.ts";
import { openDatabase } from "#app/db.ts";
import { env } from "#app/env.ts";
import { createAppRouter } from "#app/router.ts";

let db = openDatabase(env.DATABASE_URL);
let router = createAppRouter({ guestBook: createDataTableStore(db) });
let server = http.createServer(createRequestListener(router.fetch));

server.listen(env.PORT, () => {
    console.log(`Server running at http://localhost:${env.PORT}`);
});

let shuttingDown = false;

function shutdown() {
    if (shuttingDown) return;
    shuttingDown = true;
    server.close(() => db.close().finally(() => process.exit(0)));
    server.closeAllConnections();
}

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
