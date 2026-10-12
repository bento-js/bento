import { createController } from "remix/router";

import { routes } from "#app/routes.ts";
import { assets } from "#app/utils/assets.ts";

export default createController(routes, {
    actions: {
        async assets({ request }) {
            return (await assets.fetch(request)) ?? new Response("Not Found", { status: 404 });
        },
    },
});
