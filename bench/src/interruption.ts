const EXIT_CODES = { SIGINT: 130, SIGTERM: 143 } as const;

type InterruptingSignal = keyof typeof EXIT_CODES;

let received: InterruptingSignal | undefined;

export class RunInterrupted extends Error {
    constructor(signal: InterruptingSignal) {
        super(`Interrupted by ${signal}`);
    }
}

/**
 * The first SIGINT or SIGTERM runs `stopWork` and marks the run interrupted; the run then unwinds
 * through its own `finally` blocks. A second signal has no handler left, so it ends the process.
 */
export function handleInterruption(stopWork: () => void): void {
    for (let signal of ["SIGINT", "SIGTERM"] as const) {
        process.once(signal, () => {
            received ??= signal;
            stopWork();
        });
    }
}

/** Stops work that would otherwise start, or be recorded as a failure, after an interruption. */
export function throwIfInterrupted(): void {
    if (received !== undefined) throw new RunInterrupted(received);
}

/** Prints why the run ended and sets the exit status: the signal's conventional code, else 1. */
export function reportRunFailure(error: unknown): void {
    if (received !== undefined) {
        console.error(`[bench] interrupted by ${received}`);
        process.exitCode = EXIT_CODES[received];
        return;
    }
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
}
