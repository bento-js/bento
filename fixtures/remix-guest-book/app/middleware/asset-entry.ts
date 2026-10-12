import type { ScriptEntry } from "remix/assets";

import * as path from "node:path";
import { getContext } from "remix/middleware/async-context";
import { createContextKey, type Middleware } from "remix/router";

import { assets } from "#app/utils/assets.ts";

interface AssetEntry {
    scriptEntry: ScriptEntry;
    stylesheetHref: string;
}

let assetEntryKey = createContextKey<AssetEntry>();
let defaultScriptEntry = path.resolve(import.meta.dirname, "../actions/public/entry.ts");
let defaultStylesheetEntry = path.resolve(import.meta.dirname, "../actions/public/preflight.css");

/** Resolves the document's script and stylesheet entries for `getAssetEntry()`. */
export type AssetEntryMiddleware = Middleware<{ key: typeof assetEntryKey; value: AssetEntry }>;

let [resolvedScriptEntry, stylesheetHref] = await Promise.all([
    assets.getScriptEntry(defaultScriptEntry),
    assets.getHref(defaultStylesheetEntry),
]);

export function loadAssetEntry(): AssetEntryMiddleware {
    return async (context, next) => {
        context.set(assetEntryKey, { scriptEntry: resolvedScriptEntry, stylesheetHref });
        return next();
    };
}

export function getAssetEntry(): AssetEntry {
    return getContext().get(assetEntryKey);
}
