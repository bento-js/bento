<script setup lang="ts">
import { useData, withBase } from "vitepress";
import { computed } from "vue";

interface Reference {
    id: string;
    link: string | null;
}

interface Relationship {
    label: string;
    references: Reference[];
}

interface Issue {
    reference: string;
    link: string | null;
}

let { frontmatter, theme } = useData();

let meta = computed(() => {
    let fm = frontmatter.value as Record<string, unknown>;
    let id = typeof fm.id === "string" ? fm.id : null;
    let isProposal = id?.startsWith("proposal.") ?? false;
    let isRecord = isProposal || id?.startsWith("policy.") || id?.startsWith("decision.");

    return {
        id,
        isProposal,
        isRecord,
        status: typeof fm.status === "string" ? fm.status : null,
        authors:
            isProposal && Array.isArray(fm.authors)
                ? fm.authors.filter((value): value is string => typeof value === "string")
                : [],
        issues:
            isProposal && Array.isArray(fm.issues)
                ? fm.issues
                      .filter((value): value is string => typeof value === "string")
                      .map(reference => ({
                          reference,
                          link: /^https?:\/\//.test(reference) ? reference : null,
                      }))
                : [],
        establishedBy: typeof fm["established-by"] === "string" ? fm["established-by"] : null,
        supersedes: Array.isArray(fm.supersedes)
            ? fm.supersedes.filter((value): value is string => typeof value === "string")
            : [],
        describes: Array.isArray(fm.describes)
            ? fm.describes.filter((value): value is string => typeof value === "string")
            : [],
        pullRequest:
            typeof fm["pull-request"] === "string" || typeof fm["pull-request"] === "number"
                ? String(fm["pull-request"])
                : null,
    };
});

function displayStatus(status: string) {
    return status
        .split("-")
        .map(word => word.charAt(0).toUpperCase() + word.slice(1))
        .join(" ");
}

let recordLinks = computed(() => {
    let config = theme.value as Record<string, unknown>;
    let candidate = config.recordLinks;
    let links: Record<string, string> = {};

    if (!candidate || typeof candidate !== "object") return links;

    for (let [id, link] of Object.entries(candidate)) {
        if (typeof link === "string") links[id] = link;
    }

    return links;
});

let relationships = computed<Relationship[]>(() => {
    let links = recordLinks.value;
    let result: Relationship[] = [];

    if (meta.value.establishedBy) {
        result.push({
            label: "established by",
            references: [
                { id: meta.value.establishedBy, link: links[meta.value.establishedBy] ?? null },
            ],
        });
    }

    if (meta.value.supersedes.length > 0) {
        result.push({
            label: "supersedes",
            references: meta.value.supersedes.map(id => ({ id, link: links[id] ?? null })),
        });
    }

    if (meta.value.describes.length > 0) {
        result.push({
            label: "describes",
            references: meta.value.describes.map(id => ({ id, link: links[id] ?? null })),
        });
    }

    return result;
});

let pullRequestIsUrl = computed(() => /^https?:\/\//.test(meta.value.pullRequest ?? ""));
let visible = computed(
    () => meta.value.isRecord || relationships.value.length > 0 || meta.value.pullRequest !== null,
);
</script>

<template>
    <aside v-if="visible" class="record-header" aria-label="Document metadata">
        <div class="record-header__row">
            <code v-if="meta.id" class="record-header__id">{{ meta.id }}</code>
            <span
                v-if="meta.isRecord && meta.status"
                :class="['record-header__status', `record-header__status--${meta.status}`]"
            >
                {{ displayStatus(meta.status) }}
            </span>
        </div>

        <dl
            v-if="
                meta.authors.length > 0 ||
                relationships.length > 0 ||
                meta.issues.length > 0 ||
                meta.pullRequest
            "
            class="record-header__details"
        >
            <div v-if="meta.authors.length > 0" class="record-header__detail">
                <dt>authors</dt>
                <dd>
                    <span
                        v-for="(author, index) in meta.authors"
                        :key="`${author}-${index}`"
                        class="record-header__value"
                    >
                        {{ author }}
                    </span>
                </dd>
            </div>
            <div
                v-for="relationship in relationships"
                :key="relationship.label"
                class="record-header__detail"
            >
                <dt>{{ relationship.label }}</dt>
                <dd>
                    <template v-for="reference in relationship.references" :key="reference.id">
                        <a v-if="reference.link" :href="withBase(reference.link)">{{
                            reference.id
                        }}</a>
                        <code v-else>{{ reference.id }}</code>
                    </template>
                </dd>
            </div>
            <div v-if="meta.issues.length > 0" class="record-header__detail">
                <dt>issues</dt>
                <dd>
                    <template
                        v-for="(issue, index) in meta.issues"
                        :key="`${issue.reference}-${index}`"
                    >
                        <a v-if="issue.link" :href="issue.link" target="_blank" rel="noreferrer">
                            {{ issue.reference }}
                        </a>
                        <span v-else class="record-header__value">{{ issue.reference }}</span>
                    </template>
                </dd>
            </div>
            <div v-if="meta.pullRequest" class="record-header__detail">
                <dt>pull request</dt>
                <dd>
                    <a
                        v-if="pullRequestIsUrl"
                        :href="meta.pullRequest"
                        target="_blank"
                        rel="noreferrer"
                    >
                        {{ meta.pullRequest }}
                    </a>
                    <code v-else>{{ meta.pullRequest }}</code>
                </dd>
            </div>
        </dl>
    </aside>
</template>

<style scoped>
.record-header {
    margin-bottom: 1.5rem;
    padding: 0.75rem 1rem;
    border: 1px solid var(--vp-c-divider);
    border-radius: 0.5rem;
    background-color: var(--vp-c-bg-soft);
    font-size: 0.875rem;
}

.record-header__row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
}

.record-header__id,
.record-header__details code {
    padding: 0;
    background: transparent;
    font-family: var(--vp-font-family-mono);
    color: var(--vp-c-text-1);
}

.record-header__id {
    font-size: 0.875rem;
    font-weight: 500;
}

.record-header__status {
    display: inline-block;
    padding: 0.125rem 0.5rem;
    border-radius: 0.25rem;
    font-family: var(--vp-font-family-mono);
    font-size: 0.75rem;
    font-weight: 600;
    letter-spacing: 0.05em;
}

.record-header__status--draft,
.record-header__status--awaiting-implementation,
.record-header__status--proposed,
.record-header__status--active-review,
.record-header__status--returned-for-revisions {
    background-color: rgba(245, 158, 11, 0.15);
    color: rgb(180, 83, 9);
}

.record-header__status--active,
.record-header__status--accepted,
.record-header__status--implemented {
    background-color: rgba(34, 197, 94, 0.15);
    color: rgb(21, 128, 61);
}

.record-header__status--rejected,
.record-header__status--withdrawn {
    background-color: rgba(239, 68, 68, 0.15);
    color: rgb(185, 28, 28);
}

.record-header__status--superseded {
    background-color: var(--vp-c-default-soft);
    color: var(--vp-c-text-2);
}

.record-header__details {
    display: grid;
    gap: 0.375rem;
    margin: 0.5rem 0 0;
}

.record-header__detail {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.375rem;
}

.record-header__detail dt {
    color: var(--vp-c-text-3);
    font-family: var(--vp-font-family-mono);
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
}

.record-header__detail dd {
    display: flex;
    flex-wrap: wrap;
    gap: 0.375rem;
    margin: 0;
}

.record-header__detail a,
.record-header__detail code,
.record-header__value {
    font-family: var(--vp-font-family-mono);
    font-size: 0.75rem;
}

.record-header__detail a {
    color: var(--vp-c-brand-1);
    text-decoration: none;
}

.record-header__detail a:hover {
    text-decoration: underline;
}
</style>
