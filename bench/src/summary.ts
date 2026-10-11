import type {
    Environment,
    HttpEvidence,
    Metric,
    RunRecord,
    Series,
    WorkloadResult,
} from "./results-model.ts";
import type { Summary } from "./statistics.ts";

import { RUNTIME_NAMES } from "./results-model.ts";
import { summarize } from "./statistics.ts";

const MAX_LISTED_FAILURES = 5;

export function renderSummary(run: RunRecord): string {
    return [
        "# Benchmark summary",
        `Started ${run.startedAt}. Options and environment are in \`run.json\`; every sample is in the per-workload JSON files beside this one.`,
        renderEnvironment(run.environment),
        ...run.workloads.map(renderWorkload),
    ].join("\n\n");
}

function renderEnvironment(environment: Environment): string {
    let pairs = (values: Record<string, string>) =>
        Object.entries(values)
            .map(([name, value]) => `${name} ${value}`)
            .join(", ");
    return [
        "## Environment",
        `- Hardware: ${environment.hardware}`,
        `- OS: ${environment.os}`,
        `- Runtimes: ${pairs(environment.runtimes)}`,
        `- Tools: ${pairs(environment.tools)}`,
        `- Revisions: ${pairs(environment.revisions)}`,
        `- Fixture: Remix ${environment.remixVersion} with its locked package graph`,
    ].join("\n");
}

function renderWorkload(workload: WorkloadResult): string {
    let variants = [...new Set(workload.series.map(series => series.variant))];
    let sections = variants.map(variant => {
        let series = workload.series.filter(candidate => candidate.variant === variant);
        let heading = variant === "" ? [] : [`### ${variant}`];
        return [
            ...heading,
            renderTable(workload.metrics, series),
            ...renderExplanations(series),
        ].join("\n\n");
    });
    let notes = workload.notes.map(note => `- ${note}`).join("\n");
    return [
        `## ${workload.title} (\`${workload.workload}\`)`,
        workload.question,
        ...sections,
        ...(notes ? [`Notes:\n\n${notes}`] : []),
    ].join("\n\n");
}

function renderTable(metrics: Metric[], series: Series[]): string {
    let ordered = RUNTIME_NAMES.flatMap(runtime =>
        series.filter(candidate => candidate.runtime === runtime),
    );
    let rows = metrics.map(metric => {
        let label = metric.unit === "" ? metric.label : `${metric.label} (${metric.unit})`;
        return `| ${label} | ${ordered.map(candidate => renderCell(metric, candidate)).join(" | ")} |`;
    });
    return [
        `| Metric | ${ordered.map(candidate => candidate.runtime).join(" | ")} |`,
        `| --- | ${ordered.map(() => "---").join(" | ")} |`,
        ...rows,
    ].join("\n");
}

function renderCell(metric: Metric, series: Series): string {
    let { outcome } = series;
    if (outcome.status !== "measured") return outcome.status;
    let values = outcome.samples
        .map(sample => sample[metric.key])
        .filter((value): value is number => typeof value === "number" && Number.isFinite(value));
    let summary = summarize(values);
    return summary === undefined ? "–" : formatSummary(summary);
}

function formatSummary(summary: Summary): string {
    let percentiles = Object.entries(summary.percentiles)
        .map(([name, value]) => `, ${name} ${formatNumber(value)}`)
        .join("");
    let range = `${formatNumber(summary.min)}–${formatNumber(summary.max)}`;
    return `${formatNumber(summary.median)} (${range}, IQR ${formatNumber(summary.iqr)}${percentiles}, n=${summary.count})`;
}

function formatNumber(value: number): string {
    return Math.abs(value) >= 1 || value === 0 ? value.toFixed(1) : value.toFixed(3);
}

function renderExplanations(series: Series[]): string[] {
    return RUNTIME_NAMES.flatMap(runtime =>
        series
            .filter(candidate => candidate.runtime === runtime)
            .flatMap(candidate => explain(candidate)),
    );
}

function explain({ runtime, outcome }: Series): string[] {
    switch (outcome.status) {
        case "unavailable":
            return [`**${runtime}** unavailable: ${outcome.reason}`];
        case "incompatible":
            return [explainIncompatibility(runtime, outcome)];
        case "failed":
        case "measured":
            return outcome.failures.map(
                failure => `**${runtime}** trial ${failure.trial} failed: ${failure.message}`,
            );
    }
}

function explainIncompatibility(
    runtime: string,
    outcome: { check: string; failures: string[]; evidence: HttpEvidence[] },
): string {
    let listed = outcome.failures.slice(0, MAX_LISTED_FAILURES);
    let omitted = outcome.failures.length - listed.length;
    let omission = omitted > 0 ? ` (and ${omitted} more in the raw JSON)` : "";
    let evidence = outcome.evidence.slice(0, MAX_LISTED_FAILURES).map(describeResponse);
    return [
        `**${runtime}** incompatible at the ${outcome.check} check: ${listed.join("; ")}${omission}`,
        ...(evidence.length > 0 ? ["  Response evidence:", ...evidence] : []),
    ].join("\n");
}

function describeResponse(seen: HttpEvidence): string {
    let details = [
        `${seen.status}`,
        `${seen.bodyBytes} bytes`,
        seen.contentType ?? "no content type",
        ...(seen.location === null ? [] : [`Location: ${seen.location}`]),
    ];
    return `  - ${seen.method} ${seen.url} → ${details.join(", ")}`;
}
