import type { Execution } from "../../lib/model.ts";

let counts = new Intl.NumberFormat("en-US");

export function formatCount(value: number): string {
    return counts.format(value);
}

export function formatTimestamp(iso: string): string {
    let date = new Date(iso);
    if (Number.isNaN(date.getTime())) return iso;
    return `${date.toISOString().slice(0, 19).replace("T", " ")} UTC`;
}

export function formatDuration(milliseconds: number): string {
    if (milliseconds < 1000) return `${Math.round(milliseconds)} ms`;
    let seconds = milliseconds / 1000;
    if (seconds < 60) {
        return `${Number.isInteger(seconds) ? seconds : seconds.toFixed(seconds < 10 ? 2 : 1)} s`;
    }
    let minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes} min ${Math.floor(seconds % 60)} s`;
    return `${Math.floor(minutes / 60)} h ${minutes % 60} min`;
}

export function formatElapsed(startedAt: string, finishedAt: string | null): string | null {
    if (finishedAt === null) return null;
    let elapsed = Date.parse(finishedAt) - Date.parse(startedAt);
    return Number.isFinite(elapsed) && elapsed >= 0 ? formatDuration(elapsed) : null;
}

export function formatExit(execution: Execution): string {
    if (execution.signal !== null) return `signal ${execution.signal}`;
    if (execution.exitCode !== null) return `exit ${execution.exitCode}`;
    return "no exit recorded";
}

export function shortHash(hash: string): string {
    return hash.length > 12 ? hash.slice(0, 12) : hash;
}

const SHELL_SAFE = /^[\w@%+=:,./-]+$/;

export function shellQuote(argv: readonly string[]): string {
    return argv
        .map(arg => (SHELL_SAFE.test(arg) ? arg : `'${arg.replaceAll("'", `'\\''`)}'`))
        .join(" ");
}

export function withSearch(pathname: string, params: Record<string, string | number | undefined>) {
    let search = new URLSearchParams();
    for (let [name, value] of Object.entries(params)) {
        if (value !== undefined && value !== "") search.set(name, String(value));
    }
    let query = search.toString();
    return query === "" ? pathname : `${pathname}?${query}`;
}
