# Direct source

A small project that Bento runs straight from its TypeScript and TSX source, with no build step and no emitted JavaScript. `main.ts` awaits at the top level, imports the `remix` package from npm, uses TypeScript-only syntax (`satisfies`, type annotations, and an inline type import), and renders the TSX component in `menu.tsx` to an HTML string with `remix/component/server`. `tsconfig.json` selects the `react-jsx` transform with `remix/component` as its import source; that is Remix's own component runtime, not React.

## Run it

Install the locked dependencies once. This needs pnpm 12; the lockfile pins the exact version pnpm uses.

```sh
cd examples/direct-source
bento install --frozen-lockfile
```

Then run the entry point:

```sh
bento run --allow-read=. main.ts
```

It prints the rendered component, on one line:

```text
<ul><li>Tamagoyaki: 10 minutes</li><li>Onigiri: 5 minutes</li></ul>
```

Running needs only Bento: not Node, Deno, Bun, or pnpm, which need not be on `PATH`. Bento transforms the TypeScript and TSX in memory, so running writes no JavaScript, or any other file, into the project. The only grant is `--allow-read=.`, which lets Bento load the project's own modules and its installed packages. Rendering loads no native addon, so the example grants none.

`remix` depends on esbuild, whose install script this example never needs, so `pnpm-workspace.yaml` leaves it denied. That file also keeps the example a project of its own rather than part of the repository's workspace.
