// Enforces policy.0001: Bento crates use VoidZero crates, not the SWC stack, for parsing,
// transformation, and resolution.
import { execFileSync } from "node:child_process";
import path from "node:path";

export interface CargoMetadata {
    packages: Array<{
        id: string;
        name: string;
        manifest_path: string;
        dependencies: Array<{ name: string; rename: string | null; kind: string | null }>;
    }>;
    workspace_members: string[];
}

export interface DependencyViolation {
    crate: string;
    dependency: string;
    message: string;
}

const DISALLOWED: Record<string, true> = {
    deno_ast: true,
    deno_graph: true,
    "dprint-swc-ext": true,
};

export function findDisallowedDependencies(metadata: CargoMetadata): DependencyViolation[] {
    let members = new Set(metadata.workspace_members);
    return metadata.packages
        .filter(pkg => members.has(pkg.id))
        .flatMap(pkg =>
            pkg.dependencies
                .filter(({ name }) => DISALLOWED[name] || name === "swc" || name.startsWith("swc_"))
                .map(dependency => ({
                    crate: pkg.name,
                    dependency: dependency.name,
                    message: `${pkg.name} depends on ${dependency.name}; policy.0001 requires VoidZero crates (Oxc, oxc_resolver, Rolldown) for parsing, transformation, and resolution.`,
                })),
        );
}

function readWorkspaceMetadata(root: string): CargoMetadata {
    let output = execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps"], {
        cwd: root,
        encoding: "utf8",
        maxBuffer: 64 * 1024 * 1024,
    });
    return JSON.parse(output) as CargoMetadata;
}

if (import.meta.main) {
    let root = path.resolve(import.meta.dirname, "..");
    let violations = findDisallowedDependencies(readWorkspaceMetadata(root));
    for (let violation of violations) console.error(violation.message);
    if (violations.length > 0) process.exit(1);
    console.log("Crate dependencies follow policy.0001.");
}
