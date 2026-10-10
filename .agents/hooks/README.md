# Hooks

`commit-msg` holds commits made from Oh My Pi to Scoped Commits and lets every other commit through. Oh My Pi exports `OMPCODE=1` to every command it runs and an ordinary terminal does not, so the hook checks for that variable and then calls `check-scoped-commit`, which does the actual check. CI calls `check-scoped-commit` directly for every commit in a pull request.

`vp install` installs the hook: the `prepare` script runs `vp config --hooks-dir .agents/hooks`, which points `core.hooksPath` at Vite+'s generated dispatcher in `.agents/hooks/_/`, and the dispatcher runs the scripts in this directory. To install it without reinstalling dependencies:

```sh
vp hooks enable --hooks-dir .agents/hooks
```

`vp hooks status` shows whether it is active in a clone. CI runs the same validation on every pull request, so the hook is not the only line of defence.

| Lifecycle event                           | Run              |
| ----------------------------------------- | ---------------- |
| After a file edit                         | `vp fmt <file>`  |
| Before the agent declares a turn complete | `vp run check`   |
| Session start                             | Read `AGENTS.md` |

Harness lifecycle wiring is harness-specific and lives outside `.agents/`. For Oh My Pi it is `.omp/extensions/`.

`vp run check` also requires Bun for the real OMP workflow-tool discovery check, even though Vitest runs under Node. That integration check limits the Bun subprocess to 30 seconds and gives the outer test 35 seconds, rather than applying the five-second unit-test default to cold runtime startup. Loader errors and a missing `workflow` tool still fail the gate.
