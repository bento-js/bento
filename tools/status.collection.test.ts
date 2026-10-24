import { execFileSync } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { expect, onTestFinished, test } from "vite-plus/test";

let statusScript = path.join(import.meta.dirname, "status.ts");

function statusWithPullRequests(
    pullRequests: object[],
    options: { defaultBranch?: string; originHead?: string } = {},
): string {
    let root = mkdtempSync(path.join(tmpdir(), "bento-status-"));
    onTestFinished(() => rmSync(root, { force: true, recursive: true }));
    execFileSync("git", ["init", "-q", "-b", "feature", root]);
    execFileSync("git", [
        "-C",
        root,
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.com",
        "commit",
        "--allow-empty",
        "-qm",
        "fixture",
    ]);
    if (options.originHead)
        execFileSync("git", [
            "-C",
            root,
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            `refs/remotes/origin/${options.originHead}`,
        ]);
    let headRefOid = execFileSync("git", ["-C", root, "rev-parse", "HEAD"], {
        encoding: "utf8",
    }).trim();
    let bin = path.join(root, "bin");
    mkdirSync(bin);
    let gh = path.join(bin, "gh");
    writeFileSync(
        gh,
        [
            `#!${process.execPath}`,
            "let command = process.argv[2];",
            'if (command === "--version" || command === "auth") process.exit(0);',
            'if (command === "repo") console.log(JSON.stringify({defaultBranchRef: {name: process.env.TEST_DEFAULT_BRANCH}, nameWithOwner: "owner/repo"}));',
            'else if (command === "pr" && process.argv[3] === "list") {',
            '  let state = process.argv.indexOf("--state");',
            '  let head = process.argv.indexOf("--head");',
            '  console.log(process.argv[state + 1] === "all" && process.argv[head + 1] === "feature" ? process.env.TEST_PULL_REQUESTS : "[]");',
            "}",
            "else process.exit(1);",
        ].join("\n"),
    );
    chmodSync(gh, 0o755);
    mkdirSync(path.join(root, "proposals"));
    writeFileSync(
        path.join(root, "proposals", "0001-feature.md"),
        "---\nid: proposal.0001\ntitle: Feature\nstatus: implemented\npull-request: 1\n---\n",
    );
    return execFileSync(process.execPath, [statusScript], {
        cwd: root,
        encoding: "utf8",
        env: {
            ...process.env,
            PATH: `${bin}${path.delimiter}${process.env.PATH ?? ""}`,
            TEST_PULL_REQUESTS: JSON.stringify(
                pullRequests.map(pullRequest => ({
                    headRefOid,
                    headRepository: { nameWithOwner: "owner/repo" },
                    ...pullRequest,
                })),
            ),
            TEST_DEFAULT_BRANCH: options.defaultBranch ?? "main",
        },
    });
}

test("finds an implemented feature's merged PR after it leaves the open PR list", () => {
    let report = statusWithPullRequests([
        {
            number: 1,
            state: "MERGED",
            baseRefName: "main",
            isDraft: false,
            title: "Feature",
            url: "https://github.com/owner/repo/pull/1",
        },
    ]);

    expect(report).toContain("PR #1 (merged)");
    expect(report).toContain("Phase 5 — Implementation merged");
    expect(report).toMatch(/confirm.*implemented record.*merged revision/i);
});

test("does not select an arbitrary PR when a branch name has been reused", () => {
    let report = statusWithPullRequests([
        {
            number: 1,
            state: "MERGED",
            baseRefName: "main",
            isDraft: false,
            title: "Earlier",
            url: "https://github.com/owner/repo/pull/1",
        },
        {
            number: 2,
            state: "CLOSED",
            baseRefName: "main",
            isDraft: false,
            title: "Later",
            url: "https://github.com/owner/repo/pull/2",
        },
    ]);

    expect(report).toContain("PR — unknown (Multiple pull requests found for branch feature");
    expect(report).not.toContain("Implementation merged");
});

test("rejects a PR record without the URL needed to identify its remote revision", () => {
    let report = statusWithPullRequests([
        { number: 1, state: "MERGED", baseRefName: "main", isDraft: false, title: "Feature" },
    ]);

    expect(report).toContain("PR — unknown (gh returned invalid pull request data)");
    expect(report).not.toContain("Implementation merged");
});

test("does not attribute another repository's matching branch name to this checkout", () => {
    let report = statusWithPullRequests([
        {
            number: 1,
            state: "MERGED",
            baseRefName: "main",
            isDraft: false,
            title: "Fork PR",
            url: "https://github.com/owner/repo/pull/1",
            headRepository: { nameWithOwner: "fork/repo" },
        },
    ]);

    expect(report).toMatch(/PR — unknown/);
    expect(report).not.toContain("Implementation merged");
});

test("does not use a sole historical PR after the branch has been reused", () => {
    let report = statusWithPullRequests([
        {
            number: 1,
            state: "MERGED",
            baseRefName: "main",
            isDraft: false,
            title: "Old PR",
            url: "https://github.com/owner/repo/pull/1",
            headRefOid: "0000000000000000000000000000000000000000",
        },
    ]);

    expect(report).toMatch(/PR — unknown/);
    expect(report).not.toContain("Implementation merged");
});

test("does not trust an old origin default after GitHub changes the default", () => {
    let report = statusWithPullRequests(
        [
            {
                number: 1,
                state: "MERGED",
                baseRefName: "release/old",
                isDraft: false,
                title: "Feature",
                url: "https://github.com/owner/repo/pull/1",
            },
        ],
        { originHead: "release/old" },
    );

    expect(report).not.toContain("Phase 5 — Implementation merged");
    expect(report).toContain("main");
});

test("recognizes a default branch whose name contains a slash", () => {
    let report = statusWithPullRequests(
        [
            {
                number: 1,
                state: "MERGED",
                baseRefName: "release/v1",
                isDraft: false,
                title: "Feature",
                url: "https://github.com/owner/repo/pull/1",
            },
        ],
        { defaultBranch: "release/v1", originHead: "release/v1" },
    );

    expect(report).toContain("Phase 5 — Implementation merged");
    expect(report).not.toContain("not the default branch");
});
