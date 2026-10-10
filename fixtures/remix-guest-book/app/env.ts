import * as s from "remix/data-schema";
import * as coerce from "remix/data-schema/coerce";

import { parseEnv } from "#app/utils/parse-env.ts";

let Env = s.object({
    // Keep this default in sync with `db.adapter.filename` in remix.json so the app and `remix db`
    // open the same file when DATABASE_URL is unset.
    DATABASE_URL: s.defaulted(s.string(), "db/data.db"),
    PORT: s.defaulted(coerce.number(), 1612),
});

export let env = parseEnv(Env);
