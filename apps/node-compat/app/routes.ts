import { get, route } from "remix/routes";

export let routes = route({
    assets: get("/assets/*path"),
    home: get("/"),
    inventory: get("/inventory"),
    compare: get("/compare"),
    runs: {
        index: get("/runs"),
        show: get("/runs/:runId"),
        test: get("/runs/:runId/test"),
    },
});
