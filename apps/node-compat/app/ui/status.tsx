import type { Handle } from "remix/component";

import { css } from "remix/component";

import { theme } from "#app/ui/theme.tsx";

import type { InventoryResult, Outcome, RunReport } from "../../lib/model.ts";

export type Tone = "pass" | "fail" | "warn" | "neutral" | "unknown";

export const OUTCOMES: readonly Outcome[] = [
    "passed",
    "failed",
    "timeout",
    "crashed",
    "unsupported",
    "skipped",
    "not-run",
];

export const OUTCOME_LABELS: Record<Outcome, string> = {
    passed: "Passed",
    failed: "Failed",
    timeout: "Timed out",
    crashed: "Crashed",
    unsupported: "Unsupported",
    skipped: "Skipped",
    "not-run": "Not run",
};

const OUTCOME_TONES: Record<Outcome, Tone> = {
    passed: "pass",
    failed: "fail",
    crashed: "fail",
    timeout: "warn",
    unsupported: "neutral",
    skipped: "neutral",
    "not-run": "neutral",
};

export const INVENTORY_STATUSES: readonly InventoryResult["status"][] = [
    "present",
    "missing",
    "type-mismatch",
    "error",
];

export const INVENTORY_LABELS: Record<InventoryResult["status"], string> = {
    present: "Present",
    missing: "Missing",
    "type-mismatch": "Type mismatch",
    error: "Inspection error",
};

const INVENTORY_TONES: Record<InventoryResult["status"], Tone> = {
    present: "neutral",
    missing: "fail",
    "type-mismatch": "warn",
    error: "unknown",
};

const STATE_LABELS: Record<RunReport["state"], string> = {
    running: "Running",
    complete: "Complete",
    interrupted: "Interrupted",
    failed: "Collection failed",
};

const STATE_TONES: Record<RunReport["state"], Tone> = {
    running: "warn",
    complete: "neutral",
    interrupted: "warn",
    failed: "fail",
};

export interface BadgeProps {
    tone: Tone;
    label: string;
    title?: string;
}

export function Badge(handle: Handle<BadgeProps>) {
    return () => {
        let { tone, label, title } = handle.props;
        return (
            <span data-tone={tone} mix={badgeStyle} title={title}>
                {label}
            </span>
        );
    };
}

export function OutcomeBadge(handle: Handle<{ outcome: Outcome; counted?: boolean }>) {
    return () => {
        let { outcome, counted = true } = handle.props;
        let title = counted ? undefined : "Not counted: the Node control did not pass";
        return (
            <Badge
                label={OUTCOME_LABELS[outcome]}
                title={title}
                tone={counted ? OUTCOME_TONES[outcome] : "unknown"}
            />
        );
    };
}

export function ControlBadge(handle: Handle<{ outcome: Outcome }>) {
    return () => {
        let { outcome } = handle.props;
        return (
            <Badge
                label={OUTCOME_LABELS[outcome]}
                tone={outcome === "passed" ? "pass" : "unknown"}
            />
        );
    };
}

export function InventoryBadge(handle: Handle<{ status: InventoryResult["status"] }>) {
    return () => {
        let { status } = handle.props;
        return <Badge label={INVENTORY_LABELS[status]} tone={INVENTORY_TONES[status]} />;
    };
}

export function StateBadge(handle: Handle<{ state: RunReport["state"] }>) {
    return () => {
        let { state } = handle.props;
        return <Badge label={STATE_LABELS[state]} tone={STATE_TONES[state]} />;
    };
}

function tone(name: "pass" | "fail" | "warn" | "neutral") {
    return {
        color: theme.status[name].text,
        backgroundColor: theme.status[name].fill,
        borderColor: theme.status[name].line,
    };
}

let badgeStyle = css({
    display: "inline-block",
    padding: `0 ${theme.space.md}`,
    border: "1px solid",
    borderRadius: theme.radius.sm,
    fontFamily: theme.fontFamily.sans,
    fontSize: theme.fontSize.xs,
    fontWeight: theme.fontWeight.medium,
    lineHeight: "18px",
    whiteSpace: "nowrap",
    '&[data-tone="pass"]': tone("pass"),
    '&[data-tone="fail"]': tone("fail"),
    '&[data-tone="warn"]': tone("warn"),
    '&[data-tone="neutral"]': tone("neutral"),
    '&[data-tone="unknown"]': { ...tone("neutral"), borderStyle: "dashed" },
});
