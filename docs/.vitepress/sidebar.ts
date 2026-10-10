import type { DefaultTheme } from "vitepress";

import { readdirSync, readFileSync, statSync } from "node:fs";
import { extname, join } from "node:path";
import { fileURLToPath } from "node:url";

let repoRoot = fileURLToPath(new URL("../..", import.meta.url));

interface DocumentEntry {
    id: string | null;
    link: string;
    status: string | null;
    text: string;
    filename: string;
}

interface SidebarSection {
    entries: DocumentEntry[];
    items: DefaultTheme.SidebarItem[];
    firstLink: string | null;
}

interface SidebarBuild {
    guides: DefaultTheme.SidebarItem[];
    proposals: DefaultTheme.SidebarItem[];
    decisions: DefaultTheme.SidebarItem[];
    policies: DefaultTheme.SidebarItem[];
    firstGuideLink: string | null;
    firstProposalLink: string | null;
    firstDecisionLink: string | null;
    firstPolicyLink: string | null;
    recordLinks: Record<string, string>;
}

export function generateSidebar(): SidebarBuild {
    let guides = buildGuidesSection();
    let proposals = buildRecordSection("proposals");
    let decisions = buildRecordSection("decisions");
    let policies = buildRecordSection("policies");
    let recordLinks: Record<string, string> = {};

    for (let entry of [
        ...guides.entries,
        ...proposals.entries,
        ...decisions.entries,
        ...policies.entries,
    ]) {
        if (entry.id) recordLinks[entry.id] = entry.link;
    }

    return {
        guides: guides.items,
        proposals: proposals.items,
        decisions: decisions.items,
        policies: policies.items,
        firstGuideLink: guides.firstLink,
        firstProposalLink: proposals.firstLink,
        firstDecisionLink: decisions.firstLink,
        firstPolicyLink: policies.firstLink,
        recordLinks,
    };
}

function buildGuidesSection(): SidebarSection {
    let directory = "guides";
    let absDir = join(repoRoot, directory);
    let entries = listDocuments(absDir, `/${directory}/`).sort(
        (a, b) =>
            a.text.localeCompare(b.text, "en", { sensitivity: "base" }) ||
            a.link.localeCompare(b.link),
    );

    return {
        entries,
        items: entries.map(({ text, link }) => ({ text, link })),
        firstLink: sectionEntryLink(entries, absDir, `/${directory}/`),
    };
}

function buildRecordSection(directory: "proposals" | "decisions" | "policies"): SidebarSection {
    let absDir = join(repoRoot, directory);
    let entries = listDocuments(absDir, `/${directory}/`).sort((a, b) => {
        let aNumber = Number.parseInt(a.filename, 10);
        let bNumber = Number.parseInt(b.filename, 10);
        let numberOrder =
            (Number.isNaN(bNumber) ? -1 : bNumber) - (Number.isNaN(aNumber) ? -1 : aNumber);

        return numberOrder || a.text.localeCompare(b.text, "en", { sensitivity: "base" });
    });
    let current = entries.filter(entry => entry.status !== "superseded");
    let superseded = entries
        .filter(entry => entry.status === "superseded")
        .map(({ text, link }) => ({ text, link }));
    let items: DefaultTheme.SidebarItem[] = current.map(({ text, link }) => ({ text, link }));

    if (superseded.length > 0) {
        items.push({ text: "Superseded", collapsed: true, items: superseded });
    }

    return {
        entries,
        items,
        firstLink: sectionEntryLink(entries, absDir, `/${directory}/`),
    };
}

function listDocuments(absDir: string, urlPrefix: string): DocumentEntry[] {
    return readdirSyncSafe(absDir)
        .filter(name => name !== "README.md" && extname(name) === ".md")
        .filter(name => isFile(join(absDir, name)))
        .map(filename => {
            let stem = filename.slice(0, -3);
            let metadata = readMetadata(join(absDir, filename), stem);

            return {
                id: metadata.id,
                link: `${urlPrefix}${stem}`,
                status: metadata.status,
                text: metadata.title,
                filename,
            };
        });
}

function readMetadata(
    filePath: string,
    fallback: string,
): {
    id: string | null;
    status: string | null;
    title: string;
} {
    try {
        let content = readFileSync(filePath, "utf-8");
        let frontmatterMatch = content.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/);
        let frontmatter = frontmatterMatch?.[1] ?? "";
        let title = frontmatterValue(frontmatter, "title");

        return {
            id: frontmatterValue(frontmatter, "id"),
            status: frontmatterValue(frontmatter, "status"),
            title:
                title ??
                content
                    .slice(frontmatterMatch?.[0].length ?? 0)
                    .match(/^#\s+(.+)$/m)?.[1]
                    ?.trim()
                    .replace(/\s*<!--.*?-->\s*/g, "") ??
                deSlugify(fallback),
        };
    } catch {
        return { id: null, status: null, title: deSlugify(fallback) };
    }
}

function frontmatterValue(frontmatter: string, key: string): string | null {
    let value = frontmatter.match(new RegExp(`^${key}:\\s*(.+?)\\s*$`, "m"))?.[1];
    if (!value) return null;

    let trimmed = value.trim();
    let quote = trimmed.at(0);
    return quote && (quote === '"' || quote === "'") && trimmed.endsWith(quote)
        ? trimmed.slice(1, -1)
        : trimmed;
}

function deSlugify(slug: string): string {
    let words = slug.replace(/[-_.]+/g, " ");
    return words.replace(/\b\w/g, character => character.toUpperCase());
}

function sectionEntryLink(
    entries: DocumentEntry[],
    absDir: string,
    urlPrefix: string,
): string | null {
    if (entries[0]) return entries[0].link;
    return isFile(join(absDir, "README.md")) ? `${urlPrefix}README` : null;
}

function readdirSyncSafe(dir: string): string[] {
    try {
        return readdirSync(dir).filter(name => !name.startsWith(".") && name !== "node_modules");
    } catch {
        return [];
    }
}

function isFile(path: string): boolean {
    try {
        return statSync(path).isFile();
    } catch {
        return false;
    }
}
