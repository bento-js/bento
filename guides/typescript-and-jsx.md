---
id: guide.typescript-and-jsx
title: TypeScript and JSX
describes: [proposal.0001]
---

# TypeScript and JSX

## What it does

Bento runs TypeScript, TSX, and JSX source files directly. Before a module is evaluated, Bento removes type syntax and compiles JSX natively with Oxc, inside the Bento process, using the execution-relevant settings from the project's `tsconfig.json`. There is no build step, no emitted `.js` files, and no `--import remix/node-tsx` loader.

Transforming is not type checking. Bento never reports type errors: a file with a type error that is still valid syntax runs, and the mistake surfaces only when the type checker runs. Keep the project's type checker as a task (for example `"typecheck": "tsc --noEmit"` in `package.json#tasks`) and run it with `bento run typecheck`.

Every transformed module carries a source map, so stack traces and error locations point at the original line and column of the TypeScript or TSX source rather than at generated JavaScript.

## How to use it

1. Write the application in `.ts`, `.tsx`, `.mts`, `.cts`, or `.jsx` files. `.js`, `.mjs`, and `.cjs` files are evaluated as they are, without transformation.
2. Keep the project's `tsconfig.json` next to its `package.json`. Bento reads it, follows every `extends` entry (relative files, package specifiers such as `@tsconfig/node22/tsconfig.json`, and packages that export a `tsconfig.json`), and accepts comments and trailing commas. Settings in the extending file win over settings it inherits; in an `extends` array, later entries win over earlier ones.
3. For JSX, set `"jsx"` to `"react-jsx"`, `"react-jsxdev"`, or `"react"`, plus `"jsxImportSource"` (automatic runtime) or `"jsxFactory"` and `"jsxFragmentFactory"` (classic runtime). The same settings apply to `.tsx` and `.jsx` files.
4. Run `bento run server.ts`. The entry module and everything it imports are transformed on load.

A project without a `tsconfig.json` runs with TypeScript's defaults: no JSX runtime is selected (plain TypeScript still runs), imports used only as types are elided, decorators are not transformed, and class fields follow the JavaScript specification.

## Worked example

The guest-book fixture's `tsconfig.json` sets `"jsx": "react-jsx"`, `"jsxImportSource": "remix/component"`, and `"verbatimModuleSyntax": true`. Loading it produces no errors; the only options it contains that Bento does not act on are type-checker and tooling options that have no runtime meaning.

Given `app/greeting.tsx`:

```tsx
import type { User } from "#app/user.ts";

export function Greeting({ user }: { user: User }) {
    return <h1 className="title">{user.name}</h1>;
}
```

Bento evaluates the equivalent of:

```js
import { jsx } from "remix/component/jsx-runtime";

export function Greeting({ user }) {
    return jsx("h1", { className: "title", children: user.name });
}
```

The `import type` is removed, the annotation is removed, and the JSX element becomes a call into Remix's JSX runtime rather than React's. Elements with several children call `jsxs`, fragments pass `Fragment`, and only the names a file uses are imported. With `"react-jsxdev"` the import comes from `remix/component/jsx-dev-runtime` and each `jsxDEV` call carries its source location.

## Which `tsconfig.json` options Bento honors

These options change how a file executes, so Bento applies them during transformation:

| Option | Effect at execution time |
| --- | --- |
| `jsx` | `react-jsx` and `react-jsxdev` select the automatic runtime; `react` selects the classic runtime. |
| `jsxImportSource` | Module the automatic runtime imports `jsx`, `jsxs`, `Fragment`, or `jsxDEV` from (`<source>/jsx-runtime` or `<source>/jsx-dev-runtime`). Defaults to `react`. |
| `jsxFactory`, `jsxFragmentFactory` | Call targets for the classic runtime. Default to `React.createElement` and `React.Fragment`. Imports of those names are kept even when they appear only in JSX. |
| `verbatimModuleSyntax` | When `true`, only `import type`, `export type`, and inline `type` specifiers are removed; every other import stays, including ones whose bindings are unused. When `false` or unset, imports whose bindings are used only as types are elided, as TypeScript does by default. |
| `experimentalDecorators` | Transforms TypeScript's legacy decorators. The output imports its helpers from `@oxc-project/runtime`, so a project that uses decorators must have that package installed. |
| `emitDecoratorMetadata` | Adds `design:type`, `design:paramtypes`, and `design:returntype` metadata next to legacy decorators. Types that need inference fall back to `Object`. `strictNullChecks` (or `strict`) decides whether `null` and `undefined` are elided from union types in that metadata. |
| `useDefineForClassFields` | `true` keeps class fields as specified by JavaScript (`[[Define]]` semantics; uninitialized fields exist as `undefined`). `false` moves initialized fields into the constructor as assignments and removes fields that have no initializer, matching TypeScript's output. When unset, it follows `target`: `false` for an explicit target below ES2022, otherwise `true`. |
| `extends` | Followed to build the effective options. |

## Which options Bento ignores, and why

| Option | Why it has no runtime effect |
| --- | --- |
| `paths`, `baseUrl`, `rootDirs` | Bento resolves imports the way Node does: relative paths, `package.json` `imports` (`#app/*`) and `exports`, and `node_modules`. Rewriting specifiers from `paths` would make the running program disagree with every other Node-compatible runtime. Bento warns when these could change resolution; `paths` entries that start with `#` are not warned about because Node resolves those through `package.json#imports`. |
| `target` | Bento runs on a current V8, so no syntax is downleveled. `target` only decides the default of `useDefineForClassFields`. |
| `module`, `moduleResolution` | Module syntax is kept as written; whether a file is ESM or CommonJS follows its extension and the nearest `package.json#type`, as in Node. Bento warns when `module` names a format it would have converted to (`commonjs`, `amd`, `umd`, `system`, `none`). |
| `noEmit`, `allowImportingTsExtensions`, `rewriteRelativeImportExtensions` | Bento transforms in memory and loads `.ts` specifiers directly, so there is nothing to emit or rewrite. |
| `strict` and the other type-checking flags, `types`, `lib`, `skipLibCheck`, `isolatedModules`, `esModuleInterop`, `allowJs`, `checkJs`, `resolveJsonModule` | They configure the type checker or an emitter Bento does not run. |
| `preserveValueImports`, `importsNotUsedAsValues` | Removed in TypeScript 6. Bento warns and ignores them; use `verbatimModuleSyntax`. |

## Limits and edge cases

- `"jsx": "preserve"` and `"jsx": "react-native"` leave JSX in the output, which no JavaScript engine can run. Loading such a `tsconfig.json` reports an error-level diagnostic, and transforming a file that contains JSX fails with `path:line:column: compilerOptions "jsx": "preserve" leaves JSX untransformed …`, pointing at the first JSX element. Files without JSX still run. A `.tsx` or `.jsx` file that contains JSX while `jsx` is unset fails the same way, naming the setting to add.
- Standard (TC39) decorators are not transformed. Without `experimentalDecorators`, a file that uses a decorator fails with an error at the decorator's position asking for `experimentalDecorators: true`, rather than being passed through to the engine as a syntax error.
- A syntax error stops the module from loading with `path:line:column: <parser message>`; nothing is evaluated.
- A malformed `tsconfig.json`, or a malformed file in its `extends` chain, is a configuration error that names the offending file and the JSON position. An `extends` target that cannot be found, or a circular `extends`, is reported the same way. So is a `tsconfig.json` that exists but cannot be read as a file: a directory of that name, a symbolic link whose target is gone, or a file without read permission. Only a `tsconfig.json` that does not exist at all is not an error.
- Each file is transformed on its own, as with `isolatedModules`. `const enum` values are not inlined across files, and namespaces have the limits of Oxc's per-file support.
- `.mts` and `.cts` files have their types removed and keep their module syntax; Bento does not convert `import`/`export` to `require`, nor the reverse.
- `.js`, `.mjs`, and `.cjs` files that contain JSX are not transformed; rename them to `.jsx`.
