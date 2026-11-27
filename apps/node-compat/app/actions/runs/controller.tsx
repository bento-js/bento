import { createController } from "remix/router";

import { HistoryPage } from "#app/actions/runs/history-page.tsx";
import { TestPage } from "#app/actions/runs/test-page.tsx";
import { routes } from "#app/routes.ts";
import { CASE_FILTERS } from "#app/ui/behavior/results.ts";
import { BehaviorRunPage } from "#app/ui/behavior/run-page.tsx";
import { InventoryPage } from "#app/ui/inventory/inventory-page.tsx";
import { ENTRY_FILTERS } from "#app/ui/inventory/results.ts";
import { NotFoundPage } from "#app/ui/not-found.tsx";
import { readQuery } from "#app/ui/query.ts";
import { comparePrevious } from "#app/utils/runs.ts";

export default createController(routes.runs, {
    actions: {
        async index({ render, results }) {
            return render(<HistoryPage runs={await results.list()} />);
        },
        async show({ params, render, results, url }) {
            let report = await results.get(params.runId);
            if (report === null) {
                return render(
                    <NotFoundPage
                        message={`No run with ID “${params.runId}” is saved.`}
                        title="Run not found"
                    />,
                    { status: 404 },
                );
            }
            let action = routes.runs.show.href({ runId: report.id });
            if (report.kind === "inventory") {
                return render(
                    <InventoryPage
                        action={action}
                        latest={false}
                        query={readQuery(url, ENTRY_FILTERS)}
                        report={report}
                    />,
                );
            }
            return render(
                <BehaviorRunPage
                    action={action}
                    comparison={await comparePrevious(results, report, await results.list())}
                    latest={false}
                    query={readQuery(url, CASE_FILTERS)}
                    report={report}
                />,
            );
        },
        async test({ params, render, results, url }) {
            let report = await results.get(params.runId);
            let id = url.searchParams.get("id");
            let test = id === null ? undefined : report?.behavior.find(result => result.id === id);
            if (report === null || test === undefined) {
                return render(
                    <NotFoundPage
                        message={
                            report === null
                                ? `No run with ID “${params.runId}” is saved.`
                                : `This run has no behavioral result for “${id ?? ""}”.`
                        }
                        title={report === null ? "Run not found" : "Test file not found"}
                    />,
                    { status: 404 },
                );
            }
            return render(<TestPage report={report} test={test} />);
        },
    },
});
