/// The `tsconfig.json` options that change how a source file executes.
///
/// Everything else in a tsconfig configures the type checker or an emitter Bento does not
/// run; [`CompilerSettings::load`](crate::CompilerSettings::load) reports those as
/// diagnostics instead of carrying them here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompilerSettings {
    pub jsx: JsxMode,
    /// `verbatimModuleSyntax`: remove only `type` imports and exports, keeping every value
    /// import even when its bindings are unused.
    pub verbatim_module_syntax: bool,
    /// `experimentalDecorators`: transform TypeScript's legacy decorators.
    pub experimental_decorators: bool,
    /// `emitDecoratorMetadata`: emit `design:*` metadata next to legacy decorators.
    pub emit_decorator_metadata: bool,
    /// `strictNullChecks` (falling back to `strict`): whether `null` and `undefined` are
    /// elided from union types in decorator metadata.
    pub strict_null_checks: bool,
    /// `useDefineForClassFields`: `true` keeps JavaScript `[[Define]]` field semantics;
    /// `false` assigns initialized fields in the constructor and drops uninitialized ones.
    pub use_define_for_class_fields: bool,
}

impl Default for CompilerSettings {
    /// TypeScript's own defaults for a project without a `tsconfig.json`.
    fn default() -> Self {
        Self {
            jsx: JsxMode::Unavailable { configured: None },
            verbatim_module_syntax: false,
            experimental_decorators: false,
            emit_decorator_metadata: false,
            strict_null_checks: true,
            use_define_for_class_fields: true,
        }
    }
}

/// How JSX in `.tsx` and `.jsx` files is compiled, from `compilerOptions.jsx`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JsxMode {
    /// `react-jsx` or `react-jsxdev`: import `jsx`/`jsxs`/`Fragment` (or `jsxDEV`) from
    /// `<import_source>/jsx-runtime` (or `/jsx-dev-runtime`).
    Automatic { import_source: String, development: bool },
    /// `react`: call `jsxFactory` and `jsxFragmentFactory`, importing nothing.
    Classic { factory: String, fragment_factory: String },
    /// JSX cannot execute: `jsx` is `preserve`, `react-native`, an unknown value, or unset.
    /// Files without JSX still transform.
    Unavailable { configured: Option<String> },
}
