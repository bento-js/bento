---
id: policy.0001
title: Prefer VoidZero Crates for JavaScript Tooling
status: active
established-by: proposal.0001
supersedes: []
---

# Prefer VoidZero Crates for JavaScript Tooling

## Introduction

Bento reuses two upstream ecosystems that overlap: Deno's runtime crates, which bring their own SWC-based parsing and Deno's resolver, and VoidZero's toolchain (Oxc, `oxc_resolver`, Rolldown). This policy settles which one Bento's own code builds on whenever both could do the job. Bento then has one parser, one transformer, and one resolver family that behave identically in the runtime and in future tooling.

## The rule

Bento code must use VoidZero crates for parsing, transformation, source maps, minification, module resolution, and bundling wherever they provide the capability, and must not add Deno- or SWC-based crates for those roles.

## Why this exists

The first runtime design adapted Deno's standalone runtime wholesale, which would have pulled in `deno_ast` (SWC) to analyze CommonJS exports and kept Deno's resolver for every import. Meanwhile, the transformer already used Oxc. Every application module would have been parsed by two different parsers whose syntax support, error messages, and edge cases differ. tsconfig would have been read by a different implementation than the one Oxc-based tools use. Each duplicate is another place where the runtime and future Bento tooling can disagree about the same source file, and another dependency stack to upgrade.

## What it applies to

- Applies to: dependencies that Bento crates declare, and the code paths Bento owns: TypeScript and JSX transformation, tsconfig loading, CommonJS/ESM export analysis, ESM import resolution, source maps, and any future minifier, bundler, or development server.
- Does not apply to: crates that a reused upstream component requires internally. For example, `deno_node` links `deno_ast`, and its CommonJS `require` and CJS-to-ESM translation are built on `node_resolver`. Those stay until Bento owns that component. It also does not apply to the application's own npm dependencies (a fixture's `oxc-transform`, `lightningcss`, or `typescript` packages) or to capabilities no VoidZero crate offers, such as CSS (Lightning CSS) or the JavaScript engine itself.

## Enforcement

- Tier: check, with prose for role choices.
- Mechanism: `vp run check` runs `node tools/crate-dependencies.ts`. It fails when any workspace crate declares a direct dependency on `deno_ast`, `deno_graph`, `dprint-swc-ext`, or any `swc`/`swc_*` package, under any dependency kind or rename.
- Prose justification: the check catches the SWC stack, which has a VoidZero replacement for every role Bento owns. Whether a Deno crate is required or replaceable depends on what it does. A crate like `node_resolver` is legitimate when `deno_node` requires it, so reviewers judge Deno crates against this policy rather than a denylist.

## How to comply

1. Before adding a dependency for a parsing, transformation, resolution, source-map, minification, or bundling role, look for the VoidZero crate that provides it (`oxc`, `oxc_resolver`, `oxc_sourcemap`, Rolldown) and use that.
2. If only a Deno or SWC crate can fill the role, record why in the dependent crate's `Cargo.toml` next to the dependency. If the check rejects it, change this policy through a proposal rather than working around the check.
3. Run `vp run check` before submitting.

## Revision history

| Date       | Change                   | Proposal      |
| ---------- | ------------------------ | ------------- |
| 2026-10-10 | Established this policy. | proposal.0001 |
