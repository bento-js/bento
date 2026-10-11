import * as http from "node:http";
import { createRequestListener } from "remix/node-fetch-server";

import { hostStore } from "#app/data/host-store.ts";
import { env } from "#app/env.ts";
import { createAppRouter } from "#app/router.ts";

let router = createAppRouter({ guestBook: hostStore });
let server = http.createServer(createRequestListener(router.fetch));

server.listen(env.PORT, () => {
    console.log(`Server running at http://localhost:${env.PORT}`);
});

let shuttingDown = false;

function shutdown() {
    if (shuttingDown) return;
    shuttingDown = true;
    server.close(() => process.exit(0));
    server.closeAllConnections();
}

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
