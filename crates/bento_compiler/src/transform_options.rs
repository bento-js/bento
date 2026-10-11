use oxc::transformer::{DecoratorOptions, JsxOptions, JsxRuntime, TransformOptions};

use crate::settings::{CompilerSettings, JsxMode};

/// Oxc options that realize `settings`. No ECMAScript downleveling is enabled: the output
/// runs on a current V8, so only TypeScript, JSX, and legacy decorators are transformed.
pub(crate) fn transform_options(settings: &CompilerSettings) -> TransformOptions {
    let mut options = TransformOptions::default();
    options.typescript.only_remove_type_imports = settings.verbatim_module_syntax;
    options.decorator = DecoratorOptions {
        legacy: settings.experimental_decorators,
        emit_decorator_metadata: settings.emit_decorator_metadata,
        strict_null_checks: settings.strict_null_checks,
    };
    // Together these reproduce `useDefineForClassFields: false` without the class-properties
    // lowering, which would also rewrite private fields into WeakMaps.
    if !settings.use_define_for_class_fields {
        options.assumptions.set_public_class_fields = true;
        options.typescript.remove_class_fields_without_initializer = true;
    }
    options.jsx = jsx_options(&settings.jsx);
    if let JsxMode::Classic { factory, fragment_factory } = &settings.jsx {
        // Tells import elision that the factories are used by JSX even when nothing else
        // references them.
        options.typescript.jsx_pragma = factory.clone().into();
        options.typescript.jsx_pragma_frag = fragment_factory.clone().into();
    }
    options
}

fn jsx_options(mode: &JsxMode) -> JsxOptions {
    match mode {
        JsxMode::Automatic { import_source, development } => {
            let mut jsx = JsxOptions::enable();
            jsx.runtime = JsxRuntime::Automatic;
            jsx.import_source = Some(import_source.clone());
            jsx.development = *development;
            jsx.conform();
            jsx
        }
        JsxMode::Classic { factory, fragment_factory } => {
            let mut jsx = JsxOptions::enable();
            jsx.runtime = JsxRuntime::Classic;
            jsx.pragma = Some(factory.clone());
            jsx.pragma_frag = Some(fragment_factory.clone());
            jsx
        }
        JsxMode::Unavailable { .. } => JsxOptions::disable(),
    }
}
