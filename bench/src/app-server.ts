import { connect } from "node:net";
import { setTimeout as sleep } from "node:timers/promises";

import type { ExpectedEntry, ResponseSnapshot } from "./document-gate.ts";
import type { Fixture } from "./fixture.ts";
import type { ManagedProcess } from "./processes.ts";
import type { Mode, RuntimeName } from "./results-model.ts";

import { evaluateDocument } from "./document-gate.ts";
import { exchange, RequestFailure } from "./http-exchange.ts";
import { serverCommand, serverEnvironment } from "./launch-commands.ts";
import { allocatePort } from "./ports.ts";
import { describeExit, spawnManaged } from "./processes.ts";

export interface AppServer {
    runtime: RuntimeName;
    process: ManagedProcess;
    port: number;
    baseUrl: string;
    /** `performance.now()` immediately before the process was spawned. */
    spawnedAt: number;
}

export interface LaunchRequest {
    runtime: RuntimeName;
    executable: string;
    fixture: Fixture;
    mode: Mode;
    watch: boolean;
}

export class ServerStartupError extends Error {}

/** Long enough for a first start on a cold machine, short enough to report a hang. */
export const STARTUP_TIMEOUT_MS = 60_000;

/** Launches a server, runs `use`, and always stops the server's whole process group afterwards. */
export async function withServer<Result>(
    request: LaunchRequest,
    use: (server: AppServer) => Promise<Result>,
): Promise<Result> {
    let server = await launchServer(request);
    try {
        return await use(server);
    } finally {
        await server.process.stop();
    }
}

export async function launchServer(request: LaunchRequest): Promise<AppServer> {
    let port = await allocatePort();
    let command = serverCommand(request.runtime, request.executable, { watch: request.watch });
    let env = serverEnvironment({
        port,
        databaseUrl: request.fixture.databaseUrl,
        mode: request.mode,
    });
    let spawnedAt = performance.now();
    let child = spawnManaged(command, { cwd: request.fixture.directory, env });
    return {
        runtime: request.runtime,
        process: child,
        port,
        baseUrl: `http://127.0.0.1:${port}`,
        spawnedAt,
    };
}

/** Waits until the port accepts a connection; returns `performance.now()` at that moment. */
export async function waitUntilListening(server: AppServer, timeoutMs: number): Promise<number> {
    let deadline = performance.now() + timeoutMs;
    while (performance.now() < deadline) {
        failIfExited(server);
        if (await canConnect(server.port)) return performance.now();
        await sleep(1);
    }
    throw new ServerStartupError(
        `${server.runtime} did not accept connections on port ${server.port} within ${timeoutMs} ms:\n${server.process.output()}`,
    );
}

/** Polls `GET /` until the response passes the document gate; returns when and what it served. */
export async function waitForDocument(
    server: AppServer,
    expectedEntries: readonly ExpectedEntry[],
    timeoutMs: number,
): Promise<{ at: number; snapshot: ResponseSnapshot }> {
    let deadline = performance.now() + timeoutMs;
    let lastProblems = "no response";
    while (performance.now() < deadline) {
        failIfExited(server);
        try {
            let { snapshot } = await exchange(server.baseUrl, { path: "/", timeoutMs: 5_000 });
            let problems = evaluateDocument(snapshot, expectedEntries);
            if (problems.length === 0) return { at: performance.now(), snapshot };
            lastProblems = problems.join("; ");
        } catch (error) {
            if (!(error instanceof RequestFailure)) throw error;
            lastProblems = error.message;
        }
        await sleep(10);
    }
    throw new ServerStartupError(
        `${server.runtime} served no correct page within ${timeoutMs} ms; last problems: ${lastProblems}`,
    );
}

function failIfExited(server: AppServer): void {
    let status = server.process.exitStatus();
    if (status !== undefined) {
        throw new ServerStartupError(
            `${server.runtime} exited (${describeExit(status)}) before it was ready:\n${server.process.output()}`,
        );
    }
}

function canConnect(port: number): Promise<boolean> {
    return new Promise(resolve => {
        let socket = connect({ port, host: "127.0.0.1" });
        socket.once("connect", () => {
            socket.destroy();
            resolve(true);
        });
        socket.once("error", () => {
            socket.destroy();
            resolve(false);
        });
    });
}
