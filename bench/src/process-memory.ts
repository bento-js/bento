import { setTimeout as sleep } from "node:timers/promises";

import { capture } from "./executables.ts";

export interface ProcessRow {
    pid: number;
    parentPid: number;
    rssKb: number;
}

export function parseProcessTable(psOutput: string): ProcessRow[] {
    return psOutput
        .split("\n")
        .map(line => line.trim().split(/\s+/).map(Number))
        .filter(columns => columns.length === 3 && columns.every(Number.isFinite))
        .map(([pid, parentPid, rssKb]) => ({ pid: pid!, parentPid: parentPid!, rssKb: rssKb! }));
}

/** Resident kilobytes of `rootPid` plus all of its descendants. */
export function treeRssKb(table: readonly ProcessRow[], rootPid: number): number {
    let members = new Set([rootPid]);
    let grew = true;
    while (grew) {
        grew = false;
        for (let row of table) {
            if (members.has(row.pid) || !members.has(row.parentPid)) continue;
            members.add(row.pid);
            grew = true;
        }
    }
    return table.filter(row => members.has(row.pid)).reduce((total, row) => total + row.rssKb, 0);
}

export async function sampleTreeRssKb(rootPid: number): Promise<number> {
    let table = parseProcessTable(await capture("ps", ["-axo", "pid=,ppid=,rss="]));
    return treeRssKb(table, rootPid);
}

export interface RssSampler {
    /** Stops sampling and returns every sample taken, in kilobytes. */
    stop(): Promise<number[]>;
}

/** Samples the tree's resident size repeatedly until stopped. */
export function startRssSampler(rootPid: number, intervalMs: number): RssSampler {
    let samples: number[] = [];
    let running = true;
    let loop = (async () => {
        while (running) {
            samples.push(await sampleTreeRssKb(rootPid));
            await sleep(intervalMs);
        }
    })();
    return {
        async stop() {
            running = false;
            await loop;
            return samples;
        },
    };
}
