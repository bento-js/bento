import { once } from "node:events";
import { createServer } from "node:net";

const FIRST_PORT = 17_700;
const LAST_PORT = 17_799;

let nextPort = FIRST_PORT;

/** The next free port in the harness's range, rotating so a port is not reused immediately. */
export async function allocatePort(): Promise<number> {
    for (let attempt = 0; attempt <= LAST_PORT - FIRST_PORT; attempt++) {
        let port = nextPort;
        nextPort = port === LAST_PORT ? FIRST_PORT : port + 1;
        if (await isFree(port)) return port;
    }
    throw new Error(`No free port between ${FIRST_PORT} and ${LAST_PORT}`);
}

async function isFree(port: number): Promise<boolean> {
    let probe = createServer();
    probe.listen({ port, host: "::", exclusive: true });
    try {
        await once(probe, "listening");
    } catch {
        return false;
    }
    probe.close();
    await once(probe, "close");
    return true;
}
