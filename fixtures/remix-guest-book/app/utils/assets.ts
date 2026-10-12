import { createAssetServer } from "remix/assets";
import { loadConfig } from "remix/cli";

let config = await loadConfig(import.meta.dirname);
if (config.assets === undefined) throw new Error("Missing `assets` configuration in remix.json");

let isDevelopment = process.env.NODE_ENV === "development";

export let assets = createAssetServer({
    ...config.assets,
    sourceMaps: isDevelopment ? "external" : undefined,
    minify: !isDevelopment,
    fingerprint: !isDevelopment,
    // `node --watch` restarts the whole server in development, so the asset server doesn't watch.
    watch: false,
});
