import {
    mkdirSync,
    mkdtempSync,
    readFileSync,
    readdirSync,
    rmSync,
    symlinkSync,
    writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { expect, onTestFinished, test } from "vite-plus/test";

import {
    collectArtifacts,
    parseFrontmatter,
    validateArtifacts,
    withoutCode,
    type Artifact,
    type Frontmatter,
} from "./validate.ts";

function artifact(path: string, frontmatter: Frontmatter, content = ""): Artifact {
    return { path, frontmatter, content };
}

let templateDirectory = new URL("../.agents/templates/", import.meta.url);

function readTemplate(name: string): string {
    return readFileSync(new URL(name, templateDirectory), "utf8");
}

function temporaryRoot(): string {
    let root = mkdtempSync(path.join(tmpdir(), "bento-validate-"));
    onTestFinished(() => rmSync(root, { force: true, recursive: true }));
    return root;
}

function proposalSource(id = "proposal.0001"): string {
    return [
        "---",
        `id: ${id}`,
        "title: Record Validation",
        "authors: [Ada]",
        "status: draft",
        "pull-request: 42",
        "supersedes: []",
        "---",
        "",
    ].join("\n");
}

function hasClarificationViolation(content: string): boolean {
    return validateArtifacts([proposalArtifact("accepted", content)]).some(({ message }) =>
        message.includes("NEEDS CLARIFICATION"),
    );
}

function assertCodeIgnoresButProseReports(content: string): void {
    expect(hasClarificationViolation(content)).toBe(false);
    expect(
        hasClarificationViolation(`${content}\n\n[NEEDS CLARIFICATION: What remains unresolved?]`),
    ).toBe(true);
}

function validArtifacts(): Artifact[] {
    return [
        artifact("proposals/0001-records.md", {
            id: "proposal.0001",
            title: "Record Validation",
            authors: ["Ada"],
            status: "superseded",
            "pull-request": "42",
            supersedes: [],
        }),
        artifact("proposals/0002-replaces.md", {
            id: "proposal.0002",
            title: "Replacement Record",
            authors: ["Ada"],
            status: "accepted",
            "pull-request": "43",
            supersedes: ["proposal.0001"],
        }),
        artifact("policies/0001-records.md", {
            id: "policy.0001",
            title: "Record Validation Policy",
            status: "active",
            "established-by": "proposal.0002",
            supersedes: [],
        }),
        artifact("decisions/0001-records.md", {
            id: "decision.0001",
            title: "Record Validation Decision",
            status: "accepted",
            "established-by": "proposal.0002",
            supersedes: [],
        }),
        artifact("guides/records.md", {
            id: "guide.records",
            title: "Record Validation Guide",
            describes: ["proposal.0002"],
        }),
        artifact("VISION.md", {
            title: "Bento Vision",
            updated: "2026-08-25",
        }),
    ];
}
function proposalArtifact(status: string, content = ""): Artifact {
    return artifact(
        "proposals/0001-records.md",
        {
            id: "proposal.0001",
            title: "Record Validation",
            authors: ["Ada"],
            status,
            "pull-request": "42",
            supersedes: [],
        },
        content,
    );
}

test("rule 1 rejects missing required keys and invalid statuses", () => {
    let records = validArtifacts();
    records[0] = artifact("proposals/0001-records.md", {
        id: "proposal.0001",
        authors: ["Ada"],
        status: "retired",
        "pull-request": "42",
        supersedes: [],
    });
    records[1].frontmatter.supersedes = [];

    expect(validateArtifacts(records)).toStrictEqual([
        {
            path: "proposals/0001-records.md",
            message: 'missing required key "title"',
        },
        {
            path: "proposals/0001-records.md",
            message: 'invalid status "retired" for proposal',
        },
    ]);
});

test("rule 1 accepts every required key and allowed status", () => {
    expect(
        validateArtifacts(validArtifacts()).some(
            ({ message }) => message.includes("required key") || message.includes("invalid status"),
        ),
    ).toBe(false);
});

test("rule 1 requires a non-empty authors list on proposals", () => {
    let missingAuthors = validArtifacts();
    delete missingAuthors[0].frontmatter.authors;

    expect(
        validateArtifacts(missingAuthors).filter(({ message }) => message.includes("authors")),
    ).toStrictEqual([
        {
            path: "proposals/0001-records.md",
            message: 'missing required key "authors"',
        },
    ]);

    let presentAuthors = validArtifacts();
    presentAuthors[0].frontmatter.authors = ["Ada"];

    expect(
        validateArtifacts(presentAuthors).some(({ message }) => message.includes("authors")),
    ).toBe(false);
});

test("rule 1 rejects an empty authors list on proposals", () => {
    let records = validArtifacts();
    records[0].frontmatter.authors = [];

    expect(
        validateArtifacts(records).filter(({ message }) => message.includes("authors")),
    ).toStrictEqual([
        {
            path: "proposals/0001-records.md",
            message: 'key "authors" must be a non-empty inline list',
        },
    ]);
});

test.each([
    { type: "proposal", index: 1, status: "rejected" },
    { type: "proposal", index: 1, status: "withdrawn" },
    { type: "policy", index: 2, status: "draft" },
    { type: "decision", index: 3, status: "proposed" },
])("rule 1 accepts the newly allowed $type status $status", ({ type, index, status }) => {
    let records = validArtifacts();
    records[index].frontmatter.status = status;

    expect(
        validateArtifacts(records).some(
            ({ message }) => message === `invalid status "${status}" for ${type}`,
        ),
    ).toBe(false);
});

test.each([
    "draft",
    "awaiting-implementation",
    "active-review",
    "returned-for-revisions",
    "accepted",
    "implemented",
    "rejected",
    "withdrawn",
    "superseded",
])("rule 1 accepts proposal status %s", status => {
    expect(validateArtifacts([proposalArtifact(status)])).toStrictEqual([]);
});

test.each([
    { type: "proposal", index: 0, status: "active" },
    { type: "proposal", index: 0, status: "proposed" },
    { type: "policy", index: 2, status: "implemented" },
    { type: "policy", index: 2, status: "awaiting-implementation" },
    { type: "decision", index: 3, status: "awaiting-implementation" },
])("rule 1 rejects $type status $status from another record kind", ({ type, index, status }) => {
    let records = validArtifacts();
    records[index].frontmatter.status = status;

    expect(
        validateArtifacts(records).filter(
            ({ message }) => message === `invalid status "${status}" for ${type}`,
        ),
    ).toStrictEqual([
        {
            path: records[index].path,
            message: `invalid status "${status}" for ${type}`,
        },
    ]);
});

test.each([
    { kind: "absent", issues: undefined },
    { kind: "empty", issues: [] },
    { kind: "populated", issues: ["#12", "https://github.com/example/bento/issues/34"] },
])("rule 1 accepts an $kind proposal issues list", ({ issues }) => {
    let records = validArtifacts();
    if (issues !== undefined) records[0].frontmatter.issues = issues;

    expect(validateArtifacts(records).some(({ message }) => message.includes("issues"))).toBe(
        false,
    );
});

test("rule 1 rejects a proposal issues value that is not a list", () => {
    let records = validArtifacts();
    records[0].frontmatter.issues = "#12";

    expect(
        validateArtifacts(records).filter(({ message }) => message.includes("issues")),
    ).toStrictEqual([
        {
            path: "proposals/0001-records.md",
            message: 'key "issues" must be an inline list',
        },
    ]);
});

test("rule 2 rejects IDs that do not match their filenames and duplicates", () => {
    let records = validArtifacts();
    records[0].frontmatter.id = "proposal.9999";
    records[1].frontmatter.id = "policy.0001";

    expect(
        validateArtifacts(records).filter(({ message }) => message.includes("ID")),
    ).toStrictEqual([
        {
            path: "proposals/0001-records.md",
            message: 'ID "proposal.9999" must be "proposal.0001" for this filename',
        },
        {
            path: "proposals/0002-replaces.md",
            message: 'ID "policy.0001" must be "proposal.0002" for this filename',
        },
        {
            path: "proposals/0002-replaces.md",
            message: 'duplicate ID "policy.0001"; first declared in policies/0001-records.md',
        },
    ]);
});

test("rule 2 accepts IDs matching unique filenames", () => {
    expect(validateArtifacts(validArtifacts()).some(({ message }) => message.includes("ID"))).toBe(
        false,
    );
});

test("rule 3 rejects unresolved cross-references", () => {
    let records = validArtifacts();
    records[4].frontmatter.describes = ["proposal.9999"];

    expect(
        validateArtifacts(records).filter(({ message }) => message.includes("does not resolve")),
    ).toStrictEqual([
        {
            path: "guides/records.md",
            message: 'reference "proposal.9999" in describes does not resolve',
        },
    ]);
});

test("rule 3 accepts resolved cross-references", () => {
    expect(
        validateArtifacts(validArtifacts()).some(({ message }) =>
            message.includes("does not resolve"),
        ),
    ).toBe(false);
});

function policySupersession(status: string): Artifact[] {
    return [
        artifact("proposals/0001-records.md", {
            id: "proposal.0001",
            title: "Record Validation",
            authors: ["Ada"],
            status: "accepted",
            "pull-request": "42",
            supersedes: [],
        }),
        artifact("policies/0001-old.md", {
            id: "policy.0001",
            title: "Old Policy",
            status,
            "established-by": "proposal.0001",
            supersedes: [],
        }),
        artifact("policies/0002-new.md", {
            id: "policy.0002",
            title: "New Policy",
            status: "active",
            "established-by": "proposal.0001",
            supersedes: ["policy.0001"],
        }),
    ];
}

test("rule 4 rejects a superseded record that remains active", () => {
    expect(
        validateArtifacts(policySupersession("active")).filter(({ message }) =>
            message.includes("must have status"),
        ),
    ).toStrictEqual([
        {
            path: "policies/0001-old.md",
            message: 'record superseded by policy.0002 must have status "superseded"',
        },
    ]);
});

test("rule 4 accepts a record marked superseded", () => {
    expect(
        validateArtifacts(policySupersession("superseded")).some(({ message }) =>
            message.includes("must have status"),
        ),
    ).toBe(false);
});

test.each(["awaiting-implementation", "active-review", "accepted", "implemented", "superseded"])(
    "rule 5 rejects a clarification marker in a %s proposal",
    status => {
        let records = validArtifacts();
        records[1].frontmatter.status = status;
        records[1].content = "[NEEDS CLARIFICATION: What is the release boundary?]";

        expect(
            validateArtifacts(records).filter(({ message }) =>
                message.includes("NEEDS CLARIFICATION"),
            ),
        ).toStrictEqual([
            {
                path: "proposals/0002-replaces.md",
                message: `proposal status "${status}" does not allow a [NEEDS CLARIFICATION:] marker`,
            },
        ]);
    },
);

test.each(["draft", "returned-for-revisions", "rejected", "withdrawn"])(
    "rule 5 permits a clarification marker in a %s proposal",
    status => {
        let records = validArtifacts();
        records[1].frontmatter.status = status;
        records[1].content = "[NEEDS CLARIFICATION: What is the release boundary?]";

        expect(
            validateArtifacts(records).some(({ message }) =>
                message.includes("NEEDS CLARIFICATION"),
            ),
        ).toBe(false);
    },
);

test("rule 5 ignores a marker inside an inline code span", () => {
    let records = validArtifacts();
    records[1].frontmatter.status = "accepted";
    records[1].content = "Rule 5 rejects a `[NEEDS CLARIFICATION:` marker in a final proposal.";

    expect(
        validateArtifacts(records).some(({ message }) => message.includes("NEEDS CLARIFICATION")),
    ).toBe(false);
});

test("rule 5 ignores a marker inside a fenced code block", () => {
    let records = validArtifacts();
    records[1].frontmatter.status = "accepted";
    records[1].content = "Example:\n\n```md\n- [NEEDS CLARIFICATION: how?]\n```\n";

    expect(
        validateArtifacts(records).some(({ message }) => message.includes("NEEDS CLARIFICATION")),
    ).toBe(false);
});

test("rule 5 still fires on a prose marker beside a quoted one", () => {
    let records = validArtifacts();
    records[1].frontmatter.status = "accepted";
    records[1].content =
        "The `[NEEDS CLARIFICATION:` form is the convention.\n\n" +
        "- [NEEDS CLARIFICATION: is this genuinely unresolved?]\n";

    expect(
        validateArtifacts(records).some(({ message }) => message.includes("NEEDS CLARIFICATION")),
    ).toBe(true);
});

test("withoutCode strips code while preserving surrounding prose", () => {
    expect(withoutCode("before `x` after")).toBe("before  after");

    let stripped = withoutCode("before\n\n```\nexample\n```\n\nafter");
    expect(stripped).not.toContain("example");
    expect(stripped).toMatch(/^before/);
    expect(stripped).toMatch(/after$/);
});

test("a valid full record set has no violations", () => {
    expect(validateArtifacts(validArtifacts())).toStrictEqual([]);
});

test("parseFrontmatter accepts blank lines and full-line comments without stripping values", () => {
    let source = [
        "---",
        "",
        "  # A YAML comment",
        "\t",
        "id: proposal.0001",
        "title: A title # with a literal hash",
        "authors: [Ada, Grace]",
        "---",
        "Body",
    ].join("\n");

    expect(parseFrontmatter(source)).toStrictEqual({
        frontmatter: {
            id: "proposal.0001",
            title: "A title # with a literal hash",
            authors: ["Ada", "Grace"],
        },
        content: source,
    });
});

test("parseFrontmatter rejects malformed input and duplicate keys", () => {
    expect(parseFrontmatter("title: Missing delimiters")).toStrictEqual({
        error: "missing YAML frontmatter",
    });
    expect(parseFrontmatter("---\ntitle: First\ntitle: Second\n---")).toStrictEqual({
        error: 'duplicate frontmatter key "title"',
    });
    expect(parseFrontmatter("---\nnot valid\n---")).toStrictEqual({
        error: 'cannot parse frontmatter line "not valid"',
    });
});

test("every record template has parsable frontmatter", () => {
    let templates = readdirSync(templateDirectory, { withFileTypes: true })
        .filter(entry => entry.isFile())
        .map(entry => [entry.name, readTemplate(entry.name)] as const);
    let frontmatterTemplates = templates.filter(([, source]) => source.startsWith("---"));

    expect(frontmatterTemplates.map(([name]) => name).sort()).toStrictEqual([
        "DECISION.md",
        "GUIDE.md",
        "POLICY.md",
        "PROPOSAL.md",
        "VISION.md",
    ]);
    for (let [name, source] of frontmatterTemplates) {
        let parsed = parseFrontmatter(source);
        expect(parsed.error, `${name}: ${parsed.error}`).toBeUndefined();
    }
});

test("a filled-in proposal template validates", () => {
    let source = readTemplate("PROPOSAL.md")
        .replace("id: proposal.<NNNN>", "id: proposal.0001")
        .replace("title: <Title Case>", "title: Template Validation")
        .replace("authors: [<name>]", "authors: [Ada]")
        .replace("pull-request: <url or number, empty while unopened>", "pull-request: 42");
    let parsed = parseFrontmatter(source);

    expect(parsed.error).toBeUndefined();
    expect(
        validateArtifacts([
            artifact("proposals/0001-template.md", parsed.frontmatter ?? {}, parsed.content),
        ]),
    ).toStrictEqual([]);
});

test("collectArtifacts validates symlinked regular files without traversing symlinked directories", () => {
    let root = temporaryRoot();
    let proposals = path.join(root, "proposals");
    let nested = path.join(root, "nested");
    mkdirSync(proposals);
    mkdirSync(nested);
    writeFileSync(path.join(root, "record.md"), proposalSource("proposal.9999"));
    writeFileSync(path.join(nested, "hidden.md"), proposalSource());
    symlinkSync(path.join(root, "record.md"), path.join(proposals, "0001-records.md"));
    symlinkSync(nested, path.join(proposals, "nested"));

    let collected = collectArtifacts(root);

    expect(collected.violations).toStrictEqual([]);
    expect(collected.artifacts.map(({ path: artifactPath }) => artifactPath)).toStrictEqual([
        "proposals/0001-records.md",
    ]);
    expect(validateArtifacts(collected.artifacts)).toStrictEqual([
        {
            path: "proposals/0001-records.md",
            message: 'ID "proposal.9999" must be "proposal.0001" for this filename',
        },
    ]);
});

test("collectArtifacts rejects markdown nested beneath record directories", () => {
    let root = temporaryRoot();
    let nested = path.join(root, "proposals", "drafts");
    mkdirSync(nested, { recursive: true });
    writeFileSync(path.join(nested, "0001-record.md"), proposalSource());

    expect(collectArtifacts(root)).toStrictEqual({
        artifacts: [],
        violations: [
            {
                path: "proposals/drafts/0001-record.md",
                message: "nested record file; move it to proposals/ or remove it",
            },
        ],
    });
});

test("rule 2 rejects guide filenames that are not kebab-case", () => {
    let records = validArtifacts();
    records[4] = artifact("guides/My Guide (DRAFT)!.md", {
        id: "guide.My Guide (DRAFT)!",
        title: "Record Validation Guide",
        describes: ["proposal.0002"],
    });

    expect(
        validateArtifacts(records).filter(({ path: file }) => file.startsWith("guides/")),
    ).toStrictEqual([
        {
            path: "guides/My Guide (DRAFT)!.md",
            message: "guide filename must be kebab-case",
        },
    ]);
});

test("rule 4 rejects a self-supersession", () => {
    let records = policySupersession("superseded");
    records[2].frontmatter.status = "superseded";
    records[2].frontmatter.supersedes = ["policy.0002"];

    expect(
        validateArtifacts(records).filter(({ message }) => message.includes("supersession cycle")),
    ).toStrictEqual([
        {
            path: "policies/0002-new.md",
            message: "supersession cycle: policy.0002 -> policy.0002",
        },
    ]);
});

test("rule 4 rejects a supersession cycle", () => {
    let records = policySupersession("superseded");
    records[1].frontmatter.supersedes = ["policy.0002"];
    records[2].frontmatter.status = "superseded";

    expect(
        validateArtifacts(records).filter(({ message }) => message.includes("supersession cycle")),
    ).toStrictEqual([
        {
            path: "policies/0001-old.md",
            message: "supersession cycle: policy.0001 -> policy.0002 -> policy.0001",
        },
    ]);
});

test("rule 5 ignores markers inside tilde fences but detects surrounding prose", () => {
    assertCodeIgnoresButProseReports("~~~md\n[NEEDS CLARIFICATION: This is code.]\n~~~");
});

test("rule 5 respects a backtick fence's opening length", () => {
    assertCodeIgnoresButProseReports(
        "````md\n```\n[NEEDS CLARIFICATION: This is code.]\n```\n````",
    );
});

test("rule 5 does not open a fence whose info string contains a backtick", () => {
    expect(
        hasClarificationViolation("``` `note`\n[NEEDS CLARIFICATION: This remains prose.]\n```\n"),
    ).toBe(true);
});

test("rule 5 does not mistake inline code for an opening fence", () => {
    expect(hasClarificationViolation("`note`\n[NEEDS CLARIFICATION: This remains prose.]")).toBe(
        true,
    );
});
