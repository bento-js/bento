import type { Handle } from "remix/component";

import { routes } from "#app/routes.ts";
import { Document } from "#app/ui/document.tsx";
import { EmptyState } from "#app/ui/empty-state.tsx";
import { PageHeader, proseStyle } from "#app/ui/layout.tsx";

import type { RunSummary } from "../../lib/model.ts";

export interface NoRunPageProps {
    kind: "behavior" | "inventory";
    runs: RunSummary[];
}

export function NoRunPage(handle: Handle<NoRunPageProps>) {
    return () => {
        let { kind, runs } = handle.props;
        let behavior = kind === "behavior";
        let title = behavior ? "Behavior" : "Availability";
        return (
            <Document section={behavior ? "behavior" : "inventory"} title={title}>
                <PageHeader title={title}>
                    <p mix={proseStyle}>
                        {behavior
                            ? "Behavior runs execute unmodified upstream Node test files under the pinned Node control and under Bento."
                            : "Availability inventories compare globals and public built-in module exports with the pinned Node baseline by name and typeof. They do not call APIs."}
                    </p>
                </PageHeader>
                {runs.length === 0 ? (
                    <EmptyState kind="all" title="No runs recorded yet" />
                ) : (
                    <>
                        <EmptyState
                            kind={kind}
                            title={
                                behavior
                                    ? "No behavior run recorded yet"
                                    : "No availability inventory recorded yet"
                            }
                        />
                        <p mix={proseStyle}>
                            Other saved runs are listed in{" "}
                            <a href={routes.runs.index.href()}>History</a>.
                        </p>
                    </>
                )}
            </Document>
        );
    };
}
