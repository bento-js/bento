# Verification

Claiming work is complete without verification is dishonesty, not efficiency.

```
NO COMPLETION CLAIMS WITHOUT FRESH VERIFICATION EVIDENCE
```

If you have not run the verifying command **in this turn**, you cannot claim its result.

## The gate

```
1. IDENTIFY  — what command proves the claim?
2. RUN       — the full command, fresh, no shortcuts
3. READ      — full output, exit code, failure count
4. VERIFY    — does the output confirm the claim?
5. CLAIM     — only now
```

Skipping a step is lying.

## What proves what

| Claim | Required evidence | Insufficient |
| --- | --- | --- |
| Tests pass | Test command output, 0 failures, this turn | "Last run was clean", "should pass now" |
| Linter clean | Linter output, 0 errors, this turn | A partial check, or related code passing |
| Build succeeds | Build exit 0, this turn | Linter or type check passed |
| Bug fixed | The failing reproduction now passes | Code changed, "I think it's fixed" |
| Regression test works | The full red-green proof below | The test passing once against the fix |
| Subagent finished | `git status` and `git diff` show the changes | The subagent's own success report |
| Preview works | Load or install it, then exercise the changed behavior | The workflow going green |
| Proposal implemented | `mise run check` exit 0, this turn | The code looking right |
| All behavior implemented | The self-check: each Detailed design behavior traced to its test, guide, and code | Tests passing |

The red-green proof for a regression test:

```
1. Write the test
2. Run it with the fix     — must PASS
3. Revert the fix
4. Run it                  — must FAIL
5. Restore the fix
6. Run it                  — must PASS again
```

Without step 4 you do not know the test catches the bug.

## Red flags — run the command first

"Should", "probably", "seems to", "I'm pretty sure" · "Done!" before running anything · committing or opening a pull request without re-running the gates · trusting a subagent's report without reading the diff · extrapolating from a partial check · wanting the work to be over.

## Applies before

Saying done, complete, fixed, passing, ready, or shipped · any positive statement about the state of the work · committing · advancing a task · reporting a subagent's success onward · posting a readiness report · merging.

The rule covers exact phrases, paraphrases, and anything that implies success.
