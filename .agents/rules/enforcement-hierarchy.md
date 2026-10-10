# Enforcement Hierarchy

A rule an agent must remember is the weakest kind of rule. Under load, prose gets missed. Durable conventions are machine-enforced, not merely recalled. When you add a rule, reach down this hierarchy first.

## The tiers — strongest to weakest

| Tier | Mechanism | Role |
| --- | --- | --- |
| **Tier 0 — Checks** | `vp run validate` — the validator in `tools/` — CI workflows, the `commit-msg` git hook, and the harness lifecycle wiring in `.omp/extensions/` | Deterministic; runs whether or not the agent remembers. A rule that can live here must. |
| **Tier 1 — Tasks** | Vite+ commands and tasks the process invokes by name: `vp fmt`, `vp run check`, `vp run docs:build` | Deterministic once invoked, but something must invoke them. |
| **Tier 2 — Templates** | `.agents/templates/` | Shape the work so the correct thing is the path of least resistance. |
| **Tier 3 — Prose** | `AGENTS.md`, `SKILL.md` files, and these rules | Necessary for judgment that cannot be mechanized; the tier most likely to be missed. |

## The rule

- Before writing a prose rule, ask whether a check or template could enforce it deterministically. If it can, build that instead.
- Where a check already enforces a rule, do not also state it in prose — delete the prose.
- Keep judgment in prose. Good naming, the right abstraction, and a justified deviation are not cheap deterministic checks.

This hierarchy is why `completing-a-feature` requires attempting a check before writing a prose policy.

## Worked example — policies and decisions

`PROPOSAL.md` carries a “Policies and decisions checked” section. It makes checking the existing record part of the path of least resistance, instead of a policy saying “remember to check policies.” That is a Tier 2 template solution, not a Tier 3 reminder.
