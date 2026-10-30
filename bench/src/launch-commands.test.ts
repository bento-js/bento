import { describe, expect, test } from "vitest";

import { scriptCommand, serverEnvironment, serverCommand } from "./launch-commands.ts";

describe("launch paths the proposal fixes", () => {
    test("Node runs the fixture through its own remix/node-tsx path", () => {
        expect(serverCommand("node", "/bin/node", { watch: false })).toEqual({
            command: "/bin/node",
            args: ["--import", "remix/node-tsx", "server.ts"],
        });
        expect(serverCommand("node", "/bin/node", { watch: true }).args).toEqual([
            "--watch",
            "--import",
            "remix/node-tsx",
            "server.ts",
        ]);
    });

    test("Bun runs the source directly", () => {
        expect(serverCommand("bun", "/bin/bun", { watch: false }).args).toEqual(["server.ts"]);
        expect(serverCommand("bun", "/bin/bun", { watch: true }).args).toEqual([
            "--watch",
            "server.ts",
        ]);
    });

    test("Bento runs the source with exactly the fixture's grants", () => {
        let grants = [
            "--allow-read",
            "--allow-write=./db",
            "--allow-net",
            "--allow-env=PORT,DATABASE_URL,NODE_ENV",
            "--allow-addons=./node_modules",
        ];

        expect(serverCommand("bento", "/bin/bento", { watch: false }).args).toEqual([
            "run",
            ...grants,
            "server.ts",
        ]);
        expect(serverCommand("bento", "/bin/bento", { watch: true }).args).toEqual([
            "run",
            ...grants,
            "--watch",
            "server.ts",
        ]);
    });

    test("a standalone script gets only the authority it needs under Bento", () => {
        expect(scriptCommand("bento", "/bin/bento", ".bench/probe.ts", ["5"]).args).toEqual([
            "run",
            "--allow-read",
            "--allow-addons=./node_modules",
            ".bench/probe.ts",
            "5",
        ]);
        expect(scriptCommand("bun", "/bin/bun", ".bench/probe.ts", ["5"]).args).toEqual([
            ".bench/probe.ts",
            "5",
        ]);
        expect(scriptCommand("node", "/bin/node", ".bench/probe.ts", ["5"]).args).toEqual([
            "--import",
            "remix/node-tsx",
            ".bench/probe.ts",
            "5",
        ]);
    });
});

test("every runtime receives the same port, database, and mode, and nothing else of the harness's", () => {
    let environment = serverEnvironment({
        port: 17_701,
        databaseUrl: "db/bench.db",
        mode: "production",
    });

    expect(environment).toMatchObject({
        PORT: "17701",
        DATABASE_URL: "db/bench.db",
        NODE_ENV: "production",
    });
    expect(Object.keys(environment).sort()).toEqual([
        "DATABASE_URL",
        "HOME",
        "NODE_ENV",
        "PATH",
        "PORT",
        "TMPDIR",
    ]);
});
