use std::path::Path;

use oxc_resolver::CompilerOptions;

use crate::settings::{CompilerSettings, JsxMode};
use crate::{Diagnostic, Severity};

const JSX_CHOICES: &str =
    "set \"jsx\" to \"react-jsx\" (with \"jsxImportSource\"), \"react-jsxdev\", or \"react\"";

/// `module` values whose `tsc` output would no longer be the source's own module syntax.
const CONVERTED_MODULE_FORMATS: &[&str] = &["commonjs", "amd", "umd", "system", "none"];

/// Reports the options whose `tsc` behavior Bento does not reproduce at execution time.
pub(crate) fn review(
    path: &Path,
    options: &CompilerOptions,
    settings: &CompilerSettings,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut report = |severity: Severity, option: &str, message: String| {
        diagnostics.push(Diagnostic {
            severity,
            path: path.to_path_buf(),
            option: option.to_owned(),
            message,
        });
    };

    if let JsxMode::Unavailable { configured: Some(configured) } = &settings.jsx {
        report(Severity::Error, "jsx", unexecutable_jsx(configured));
    }
    if options.base_url.is_some() {
        report(
            Severity::Warning,
            "baseUrl",
            "\"baseUrl\" does not affect how Bento resolves imports; bare specifiers resolve from node_modules and package.json as in Node".to_owned(),
        );
    }
    let aliases = foreign_path_aliases(options);
    if !aliases.is_empty() {
        report(
            Severity::Warning,
            "paths",
            format!(
                "\"paths\" aliases {} are not applied at runtime; imports resolve as in Node, so declare runtime aliases under package.json \"imports\" (\"#name/*\")",
                aliases.join(", ")
            ),
        );
    }
    if options.root_dirs.is_some() {
        report(
            Severity::Warning,
            "rootDirs",
            "\"rootDirs\" does not affect how Bento resolves imports; relative specifiers resolve from the importing file as in Node".to_owned(),
        );
    }
    if let Some(module) = options.module.as_deref()
        && CONVERTED_MODULE_FORMATS.contains(&module.to_ascii_lowercase().as_str())
    {
        report(
            Severity::Warning,
            "module",
            format!(
                "\"module\": \"{module}\" is not applied; Bento keeps each file's module syntax and takes its format from the extension and package.json \"type\""
            ),
        );
    }
    for (option, set) in [
        ("preserveValueImports", options.preserve_value_imports.is_some()),
        ("importsNotUsedAsValues", options.imports_not_used_as_values.is_some()),
    ] {
        if set {
            report(
                Severity::Warning,
                option,
                format!(
                    "\"{option}\" was removed in TypeScript 6.0 and is ignored; use \"verbatimModuleSyntax\""
                ),
            );
        }
    }
    diagnostics
}

fn unexecutable_jsx(configured: &str) -> String {
    match configured {
        "preserve" | "react-native" => format!(
            "\"jsx\": \"{configured}\" leaves JSX untransformed, so .tsx and .jsx files cannot run; {JSX_CHOICES}"
        ),
        _ => format!("\"jsx\": \"{configured}\" is not a JSX mode; {JSX_CHOICES}"),
    }
}

/// `paths` keys Node cannot resolve on its own. Keys starting with `#` are subpath imports,
/// which the runtime resolves through `package.json#imports`.
fn foreign_path_aliases(options: &CompilerOptions) -> Vec<&str> {
    options
        .paths
        .iter()
        .flat_map(|paths| paths.keys())
        .map(String::as_str)
        .filter(|alias| !alias.starts_with('#'))
        .collect()
}
