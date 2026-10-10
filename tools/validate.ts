import { existsSync, readdirSync, readFileSync, statSync, type Dirent } from "node:fs";
import path from "node:path";

type RecordType = "proposal" | "policy" | "decision" | "guide";
type ArtifactType = RecordType | "vision";

export type FrontmatterValue = string | string[];
export type Frontmatter = Record<string, FrontmatterValue>;

export interface Artifact {
    path: string;
    frontmatter: Frontmatter;
    content: string;
}

export interface Violation {
    path: string;
    message: string;
}

export type ParsedFrontmatter =
    | { error: string; frontmatter?: undefined; content?: undefined }
    | { error?: undefined; frontmatter: Frontmatter; content: string };

interface Schema {
    required: readonly string[];
    lists: readonly string[];
    optionalLists?: readonly string[];
    nonEmptyLists?: readonly string[];
    statuses?: readonly string[];
}

const RECORD_TYPES: Record<string, RecordType> = {
    proposals: "proposal",
    policies: "policy",
    decisions: "decision",
    guides: "guide",
};

const SCHEMAS: Record<ArtifactType, Schema> = {
    proposal: {
        required: ["id", "title", "authors", "status", "pull-request", "supersedes"],
        lists: ["authors", "supersedes"],
        optionalLists: ["issues"],
        nonEmptyLists: ["authors"],
        statuses: [
            "draft",
            "awaiting-implementation",
            "active-review",
            "returned-for-revisions",
            "accepted",
            "implemented",
            "rejected",
            "withdrawn",
            "superseded",
        ],
    },
    policy: {
        required: ["id", "title", "status", "established-by", "supersedes"],
        lists: ["supersedes"],
        statuses: ["draft", "active", "superseded"],
    },
    decision: {
        required: ["id", "title", "status", "established-by", "supersedes"],
        lists: ["supersedes"],
        statuses: ["proposed", "accepted", "superseded"],
    },
    guide: {
        required: ["id", "title", "describes"],
        lists: ["describes"],
    },
    vision: {
        required: ["title", "updated"],
        lists: [],
    },
};

// Proposals in these statuses have a finished design, so an open question is a defect.
const FINAL_PROPOSAL_STATUSES: readonly string[] = [
    "awaiting-implementation",
    "active-review",
    "accepted",
    "implemented",
    "superseded",
];

function violation(file: string, message: string): Violation {
    return { path: file, message };
}

function artifactType(file: string): ArtifactType | undefined {
    let normalized = file.split(path.sep).join("/");
    if (normalized === "VISION.md") return "vision";
    let directory = normalized.split("/").at(-2);
    return directory === undefined ? undefined : RECORD_TYPES[directory];
}

function expectedId(file: string, type: RecordType): string | null {
    let filename = path.basename(file, ".md");
    if (type === "guide") {
        return /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(filename) ? `guide.${filename}` : null;
    }

    let match = filename.match(/^(\d{4})-[a-z0-9]+(?:-[a-z0-9]+)*$/);
    return match ? `${type}.${match[1]}` : null;
}

function referencesFor(artifact: Artifact, type: ArtifactType): { field: string; id: string }[] {
    let { frontmatter } = artifact;
    let fields =
        type === "guide"
            ? ["describes"]
            : type === "vision"
              ? []
              : ["supersedes", "established-by"];

    return fields.flatMap(field => {
        let value = frontmatter[field];
        if (field === "established-by") {
            return typeof value === "string" ? [{ field, id: value }] : [];
        }
        return Array.isArray(value) ? value.map(id => ({ field, id })) : [];
    });
}

// A proposal that documents the clarification convention necessarily writes the marker down.
// Only prose counts as an unresolved question, so fenced blocks and inline spans are stripped
// before the check. An author raising a real question writes it as prose, not as code.
export function withoutCode(markdown: string): string {
    let parts = markdown.split(/(\r?\n)/);
    let fence: { character: string; length: number } | undefined;

    for (let index = 0; index < parts.length; index += 2) {
        let line = parts[index];
        if (fence) {
            let closing = line.match(/^ {0,3}(`+|~+)[ \t]*$/);
            if (closing && closing[1][0] === fence.character && closing[1].length >= fence.length) {
                fence = undefined;
            } else {
                parts[index + 1] = "";
            }
            parts[index] = "";
            continue;
        }

        let opening = line.match(/^ {0,3}(`{3,}|~{3,})(.*)$/);
        if (opening && !opening[2].includes("`")) {
            fence = { character: opening[1][0], length: opening[1].length };
            parts[index] = "";
            parts[index + 1] = "";
            continue;
        }

        parts[index] = line.replace(/`[^`\n]*`/g, "");
    }

    return parts.join("");
}

export function parseFrontmatter(source: string): ParsedFrontmatter {
    let lines = source.split(/\r?\n/);
    if (lines[0] !== "---") return { error: "missing YAML frontmatter" };

    let end = lines.indexOf("---", 1);
    if (end === -1) return { error: "unterminated YAML frontmatter" };

    let frontmatter: Frontmatter = {};
    for (let line of lines.slice(1, end)) {
        let trimmed = line.trim();
        if (trimmed === "" || trimmed.startsWith("#")) continue;

        let match = line.match(/^([a-z][a-z-]*):[ \t]*(.*)$/);
        if (!match) return { error: `cannot parse frontmatter line "${line}"` };

        let [, key, rawValue] = match;
        if (Object.hasOwn(frontmatter, key)) return { error: `duplicate frontmatter key "${key}"` };

        if (!rawValue.startsWith("[")) {
            frontmatter[key] = rawValue;
            continue;
        }

        if (!rawValue.endsWith("]")) return { error: `cannot parse list value for "${key}"` };
        let contents = rawValue.slice(1, -1).trim();
        if (contents.includes("[") || contents.includes("]"))
            return { error: `cannot parse list value for "${key}"` };

        let values = contents === "" ? [] : contents.split(",").map(value => value.trim());
        if (values.some(value => value === ""))
            return { error: `cannot parse list value for "${key}"` };
        frontmatter[key] = values;
    }

    return { frontmatter, content: source };
}

export function validateArtifacts(inputArtifacts: readonly Artifact[]): Violation[] {
    let artifacts = [...inputArtifacts].sort((left, right) => left.path.localeCompare(right.path));
    let violations: Violation[] = [];
    let ids = new Map<string, Artifact>();

    for (let artifact of artifacts) {
        let type = artifactType(artifact.path);
        if (!type) continue;

        let schema = SCHEMAS[type];
        let { frontmatter } = artifact;
        for (let key of schema.required) {
            if (!Object.hasOwn(frontmatter, key) || frontmatter[key] === undefined) {
                violations.push(violation(artifact.path, `missing required key "${key}"`));
            }
        }
        for (let key of schema.required) {
            if (
                Object.hasOwn(frontmatter, key) &&
                !schema.lists.includes(key) &&
                typeof frontmatter[key] !== "string"
            ) {
                violations.push(violation(artifact.path, `key "${key}" must be a scalar`));
            }
        }
        for (let key of [...schema.lists, ...(schema.optionalLists ?? [])]) {
            if (Object.hasOwn(frontmatter, key) && !Array.isArray(frontmatter[key])) {
                violations.push(violation(artifact.path, `key "${key}" must be an inline list`));
            }
        }
        for (let key of schema.nonEmptyLists ?? []) {
            let value = frontmatter[key];
            if (Array.isArray(value) && value.length === 0) {
                violations.push(
                    violation(artifact.path, `key "${key}" must be a non-empty inline list`),
                );
            }
        }
        let { status, updated, id } = frontmatter;
        if (
            schema.statuses &&
            Object.hasOwn(frontmatter, "status") &&
            !(typeof status === "string" && schema.statuses.includes(status))
        ) {
            violations.push(
                violation(artifact.path, `invalid status "${String(status)}" for ${type}`),
            );
        }
        if (
            type === "vision" &&
            Object.hasOwn(frontmatter, "updated") &&
            !/^\d{4}-\d{2}-\d{2}$/.test(String(updated))
        ) {
            violations.push(violation(artifact.path, 'key "updated" must use YYYY-MM-DD'));
        }

        if (type === "vision" || typeof id !== "string") continue;

        let expected = expectedId(artifact.path, type);
        if (expected === null) {
            violations.push(
                violation(
                    artifact.path,
                    type === "guide"
                        ? "guide filename must be kebab-case"
                        : "filename must begin with a four-digit record number and slug",
                ),
            );
        } else if (id !== expected) {
            violations.push(
                violation(artifact.path, `ID "${id}" must be "${expected}" for this filename`),
            );
        }

        let first = ids.get(id);
        if (first) {
            violations.push(
                violation(artifact.path, `duplicate ID "${id}"; first declared in ${first.path}`),
            );
        } else {
            ids.set(id, artifact);
        }
    }

    for (let artifact of artifacts) {
        let type = artifactType(artifact.path);
        if (!type) continue;

        for (let { field, id } of referencesFor(artifact, type)) {
            let target = ids.get(id);
            if (!target) {
                violations.push(
                    violation(artifact.path, `reference "${id}" in ${field} does not resolve`),
                );
            } else if (field === "supersedes" && target.frontmatter.status !== "superseded") {
                violations.push(
                    violation(
                        target.path,
                        `record superseded by ${String(artifact.frontmatter.id)} must have status "superseded"`,
                    ),
                );
            }
        }

        let { status } = artifact.frontmatter;
        if (
            type === "proposal" &&
            typeof status === "string" &&
            FINAL_PROPOSAL_STATUSES.includes(status) &&
            withoutCode(artifact.content).includes("[NEEDS CLARIFICATION:")
        ) {
            violations.push(
                violation(
                    artifact.path,
                    `proposal status "${status}" does not allow a [NEEDS CLARIFICATION:] marker`,
                ),
            );
        }
    }

    let graph = new Map<string, string[]>();
    for (let [id, artifact] of ids) {
        let { supersedes } = artifact.frontmatter;
        graph.set(
            id,
            Array.isArray(supersedes) ? supersedes.filter(target => ids.has(target)) : [],
        );
    }

    let states = new Map<string, "visiting" | "visited">();
    let visit = (id: string, trail: string[]): void => {
        states.set(id, "visiting");
        for (let target of graph.get(id) ?? []) {
            if (states.get(target) === "visiting") {
                let cycle = [...trail.slice(trail.indexOf(target)), target];
                // The graph only holds targets present in `ids`.
                violations.push(
                    violation(ids.get(target)!.path, `supersession cycle: ${cycle.join(" -> ")}`),
                );
            } else if (!states.has(target)) {
                visit(target, [...trail, target]);
            }
        }
        states.set(id, "visited");
    };

    for (let id of graph.keys()) {
        if (!states.has(id)) visit(id, [id]);
    }

    return violations;
}

export function collectArtifacts(root = process.cwd()): {
    artifacts: Artifact[];
    violations: Violation[];
} {
    let artifacts: Artifact[] = [];
    let violations: Violation[] = [];
    let add = (relativePath: string): void => {
        let source = readFileSync(path.join(root, relativePath), "utf8");
        let parsed = parseFrontmatter(source);
        if (parsed.error !== undefined) {
            violations.push(violation(relativePath, parsed.error));
        } else {
            artifacts.push({ path: relativePath, ...parsed });
        }
    };
    let isRegularFile = (entry: Dirent, absolutePath: string): boolean => {
        if (entry.isFile()) return true;
        if (!entry.isSymbolicLink()) return false;
        try {
            return statSync(absolutePath).isFile();
        } catch {
            return false;
        }
    };
    let collectDirectory = (directory: string, relativeDirectory: string, nested = false): void => {
        let absoluteDirectory = path.join(root, relativeDirectory);
        for (let entry of readdirSync(absoluteDirectory, { withFileTypes: true })) {
            let relativePath = path.join(relativeDirectory, entry.name);
            let absolutePath = path.join(root, relativePath);
            if (entry.isDirectory()) {
                collectDirectory(directory, relativePath, true);
            } else if (entry.name.endsWith(".md") && isRegularFile(entry, absolutePath)) {
                if (nested) {
                    violations.push(
                        violation(
                            relativePath,
                            `nested record file; move it to ${directory}/ or remove it`,
                        ),
                    );
                } else if (entry.name !== "README.md") {
                    add(relativePath);
                }
            }
        }
    };

    if (existsSync(path.join(root, "VISION.md"))) add("VISION.md");
    for (let directory of Object.keys(RECORD_TYPES)) {
        let absoluteDirectory = path.join(root, directory);
        if (existsSync(absoluteDirectory)) collectDirectory(directory, directory);
    }

    return { artifacts, violations };
}

function main(): void {
    let json = process.argv.slice(2).includes("--json");
    let collected = collectArtifacts();
    let violations = [...collected.violations, ...validateArtifacts(collected.artifacts)];

    if (json) {
        console.log(JSON.stringify({ violations, count: violations.length }));
    } else if (violations.length > 0) {
        for (let { path: file, message } of violations) console.log(`${file}: ${message}`);
        console.log(`${violations.length} violation${violations.length === 1 ? "" : "s"}`);
    } else {
        console.log("Validation passed: 0 violations");
    }

    if (violations.length > 0) process.exitCode = 1;
}

if (import.meta.main) main();
