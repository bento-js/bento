import { createController } from "remix/router";

import { ComparePage } from "#app/actions/compare/page.tsx";
import { routes } from "#app/routes.ts";
import { CASE_FILTERS } from "#app/ui/behavior/results.ts";
import { BehaviorRunPage } from "#app/ui/behavior/run-page.tsx";
import { InventoryPage } from "#app/ui/inventory/inventory-page.tsx";
import { ENTRY_FILTERS } from "#app/ui/inventory/results.ts";
import { NoRunPage } from "#app/ui/no-run-page.tsx";
import { NotFoundPage } from "#app/ui/not-found.tsx";
import { readQuery } from "#app/ui/query.ts";
import { assets } from "#app/utils/assets.ts";
import { comparePrevious, loadLatest } from "#app/utils/runs.ts";

import { compareRuns } from "../../lib/comparison.ts";

export default createController(routes, {
    actions: {
        async assets({ request }) {
            return (await assets.fetch(request)) ?? new Response("Not Found", { status: 404 });
        },
        async home({ render, results, url }) {
            let { runs, report } = await loadLatest(results, "behavior");
            if (report === null) return render(<NoRunPage kind="behavior" runs={runs} />);
            return render(
                <BehaviorRunPage
                    action={routes.home.href()}
                    comparison={await comparePrevious(results, report, runs)}
                    latest
                    query={readQuery(url, CASE_FILTERS)}
                    report={report}
                />,
            );
        },
        async inventory({ render, results, url }) {
            let { runs, report } = await loadLatest(results, "inventory");
            if (report === null) return render(<NoRunPage kind="inventory" runs={runs} />);
            return render(
                <InventoryPage
                    action={routes.inventory.href()}
                    latest
                    query={readQuery(url, ENTRY_FILTERS)}
                    report={report}
                />,
            );
        },
        async compare({ render, results, url }) {
            let runs = (await results.list()).filter(run => run.kind === "behavior");
            let currentId = url.searchParams.get("current");
            let previousId = url.searchParams.get("previous");
            if (currentId === null || previousId === null) {
                return render(
                    <ComparePage
                        current={currentId ?? runs[0]?.id ?? ""}
                        previous={previousId ?? runs[1]?.id ?? ""}
                        result={null}
                        runs={runs}
                    />,
                );
            }
            let [current, previous] = await Promise.all([
                results.get(currentId),
                results.get(previousId),
            ]);
            if (current === null || previous === null) {
                let missing = current === null ? currentId : previousId;
                return render(
                    <NotFoundPage
                        message={`No run with ID “${missing}” is saved.`}
                        title="Run not found"
                    />,
                    { status: 404 },
                );
            }
            return render(
                <ComparePage
                    current={currentId}
                    previous={previousId}
                    result={{ current, previous, comparison: compareRuns(current, previous) }}
                    runs={runs}
                />,
            );
        },
    },
});
