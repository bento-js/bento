import { readFile } from "node:fs/promises";
import path from "node:path";

export async function readPackageVersion(packageDirectory: string): Promise<string> {
    let manifestPath = path.join(packageDirectory, "package.json");
    let manifest: unknown = JSON.parse(await readFile(manifestPath, "utf8"));
    if (
        typeof manifest === "object" &&
        manifest !== null &&
        "version" in manifest &&
        typeof manifest.version === "string"
    ) {
        return manifest.version;
    }
    throw new Error(`${manifestPath} has no version`);
}
