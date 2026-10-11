import type { ResponseSnapshot } from "./document-gate.ts";
import type { HttpEvidence } from "./results-model.ts";

import { lookup } from "./json-values.ts";

export interface Exchange {
    snapshot: ResponseSnapshot;
    evidence: HttpEvidence;
}

export interface RequestSpec {
    path: string;
    method?: "GET" | "POST";
    form?: Record<string, string>;
    timeoutMs?: number;
}

const DEFAULT_TIMEOUT_MS = 10_000;
const EXCERPT_CHARACTERS = 300;

export class RequestFailure extends Error {}

/** One request, redirects not followed, with the response read in full. */
export async function exchange(baseUrl: string, spec: RequestSpec): Promise<Exchange> {
    let method = spec.method ?? "GET";
    let url = new URL(spec.path, baseUrl).href;
    try {
        let response = await fetch(url, {
            method,
            redirect: "manual",
            signal: AbortSignal.timeout(spec.timeoutMs ?? DEFAULT_TIMEOUT_MS),
            ...(spec.form && {
                body: new URLSearchParams(spec.form),
                headers: { "content-type": "application/x-www-form-urlencoded" },
            }),
        });
        let body = await response.text();
        let headers = Object.fromEntries(response.headers);
        return {
            snapshot: { status: response.status, headers, body },
            evidence: {
                method,
                url,
                status: response.status,
                contentType: headers["content-type"] ?? null,
                location: headers.location ?? null,
                bodyBytes: Buffer.byteLength(body),
                bodyExcerpt: body.slice(0, EXCERPT_CHARACTERS),
            },
        };
    } catch (error) {
        throw new RequestFailure(`${method} ${spec.path} failed: ${describeFailure(error)}`);
    }
}

function describeFailure(error: unknown): string {
    if (!(error instanceof Error)) return String(error);
    let code = lookup(error.cause, "code");
    return typeof code === "string" ? `${error.message} (${code})` : error.message;
}
