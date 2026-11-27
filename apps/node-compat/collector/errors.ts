export class SetupError extends Error {
    override name = "SetupError";
}

export function describeError(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
}

export function errorCode(error: unknown): string | undefined {
    return error instanceof Error && "code" in error && typeof error.code === "string"
        ? error.code
        : undefined;
}

export function errorOutput(error: unknown): string {
    return error instanceof Error && "stderr" in error && typeof error.stderr === "string"
        ? error.stderr.trim()
        : "";
}
