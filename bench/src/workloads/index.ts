import type { WorkloadName } from "../results-model.ts";
import type { Workload } from "../workload.ts";

import { assetOperations } from "./asset-operations.ts";
import { assets } from "./assets.ts";
import { coldLaunch } from "./cold-launch.ts";
import { memory } from "./memory.ts";
import { steadyState } from "./steady-state.ts";
import { watchReload } from "./watch-reload.ts";

export const WORKLOADS: Record<WorkloadName, Workload> = {
    "cold-launch": coldLaunch,
    assets,
    "asset-operations": assetOperations,
    "steady-state": steadyState,
    "watch-reload": watchReload,
    memory,
};
