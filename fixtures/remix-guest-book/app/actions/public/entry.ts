import { run } from "remix/component";
import {
    detectMultipleImportMapSupport,
    importModule,
    preloadShim,
} from "remix/multiple-import-maps-polyfill";

let app = run({
    async loadModule(moduleUrl, exportName) {
        let mod = await importModule(moduleUrl);
        let exported = mod[exportName];

        if (typeof exported !== "function") {
            throw new TypeError(
                `Expected export '${exportName}' from '${moduleUrl}' to be a function`,
            );
        }

        return exported;
    },
    async processClientEntryPreloads(preloads) {
        if (await detectMultipleImportMapSupport()) return preloads;

        preloadShim(preloads);
        return [];
    },
});

app.ready().catch((error: unknown) => {
    console.error("Frame adoption failed:", error);
});
