import * as http from "node:http";
import { createRequestListener } from "remix/node-fetch-server";

import { createAppRouter } from "#app/router.ts";

import { openStore } from "./data/store.ts";

const HOST = "127.0.0.1";

function readPort(value: string | undefined): number {
    if (value === undefined || value === "") return 4318;
    if (!/^\d+$/.test(value) || Number(value) > 65535) {
        throw new Error(`PORT must be an integer from 0 to 65535, not ${JSON.stringify(value)}.`);
    }
    return Number(value);
}

let port = readPort(process.env.PORT);
let results = await openStore();
let router = createAppRouter({ results });
let server = http.createServer(createRequestListener(router.fetch));

server.on("error", error => {
    console.error(`Could not serve the dashboard on ${HOST}:${port}: ${error.message}`);
    results.close().finally(() => process.exit(1));
});

server.listen(port, HOST, () => {
    let address = server.address();
    let bound = typeof address === "object" && address !== null ? address.port : port;
    console.log(`Bento Node compatibility dashboard: http://${HOST}:${bound}`);
});

let shuttingDown = false;

function shutdown() {
    if (shuttingDown) return;
    shuttingDown = true;
    server.close(() => results.close().finally(() => process.exit(0)));
    server.closeAllConnections();
}

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
