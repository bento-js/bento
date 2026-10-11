use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use deno_core::{
    FastString, ModuleLoadOptions, ModuleLoadReferrer, ModuleLoadResponse, ModuleLoader,
    ModuleSource, ModuleSourceCode, ModuleSpecifier, ModuleType, ResolutionKind,
};
use deno_error::JsErrorBox;
use deno_lib::worker::{CreateModuleLoaderResult, ModuleLoaderFactory};
use deno_media_type::MediaType;
use deno_node::NodeRequireLoader;
use deno_runtime::deno_permissions::{OpenAccessKind, PermissionsContainer};
use deno_runtime::deno_web::Blob;
use node_resolver::errors::PackageJsonLoadError;
use oxc_resolver::{ResolveOptions, Resolver};

use crate::node::NodeServices;

pub(crate) struct LoaderFactory {
    pub node: NodeServices,
    pub cwd: PathBuf,
}

impl LoaderFactory {
    fn create(&self, permissions: PermissionsContainer) -> CreateModuleLoaderResult {
        let loader = Rc::new(Loader {
            node: self.node.clone(),
            permissions,
            cwd: self.cwd.clone(),
            resolver: Resolver::new(ResolveOptions {
                condition_names: vec!["node".into(), "import".into(), "default".into()],
                fully_specified: true,
                node_path: false,
                builtin_modules: true,
                ..Default::default()
            }),
        });
        CreateModuleLoaderResult {
            module_loader: loader.clone(),
            node_require_loader: loader,
            hook_registry: None,
        }
    }
}

impl ModuleLoaderFactory for LoaderFactory {
    fn create_for_main(&self, permissions: PermissionsContainer) -> CreateModuleLoaderResult {
        self.create(permissions)
    }
    fn create_for_worker(
        &self,
        _parent: PermissionsContainer,
        permissions: PermissionsContainer,
        _blob: Option<(ModuleSpecifier, Arc<Blob>)>,
    ) -> CreateModuleLoaderResult {
        self.create(permissions)
    }
}

struct Loader {
    node: NodeServices,
    permissions: PermissionsContainer,
    resolver: Resolver,
    cwd: PathBuf,
}

impl ModuleLoader for Loader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: ResolutionKind,
    ) -> Result<ModuleSpecifier, JsErrorBox> {
        if matches!(specifier, "bento:compat" | "bento:subprocess") {
            return ModuleSpecifier::parse(specifier).map_err(JsErrorBox::from_err);
        }
        let builtin = specifier.strip_prefix("node:").unwrap_or(specifier);
        if builtin == "child_process" && referrer != "bento:subprocess" {
            return ModuleSpecifier::parse("bento:subprocess").map_err(JsErrorBox::from_err);
        }
        if deno_node::is_builtin_node_module(builtin) {
            return ModuleSpecifier::parse(&format!("node:{builtin}"))
                .map_err(JsErrorBox::from_err);
        }
        if let Ok(url) = ModuleSpecifier::parse(specifier) {
            if url.scheme() != "file" {
                return Err(resolution_error(
                    specifier,
                    referrer,
                    "remote and unregistered module schemes are unavailable",
                ));
            }
            return Ok(url);
        }
        let base = ModuleSpecifier::parse(referrer)
            .ok()
            .and_then(|url| url.to_file_path().ok())
            .and_then(|path| path.parent().map(Path::to_owned))
            .unwrap_or_else(|| self.cwd.clone());
        let resolved = self
            .resolver
            .resolve(&base, specifier)
            .map_err(|error| resolution_error(specifier, referrer, &error.to_string()))?;
        ModuleSpecifier::from_file_path(resolved.path()).map_err(|_| {
            resolution_error(specifier, referrer, "resolved path is not an absolute file")
        })
    }

    fn load(
        &self,
        specifier: &ModuleSpecifier,
        _referrer: Option<&ModuleLoadReferrer>,
        _options: ModuleLoadOptions,
    ) -> ModuleLoadResponse {
        if specifier.as_str() == "bento:compat" {
            return ModuleLoadResponse::Sync(Ok(source(
                specifier,
                FastString::from_static(include_str!("compat.mjs")),
                ModuleType::JavaScript,
            )));
        }
        if specifier.as_str() == "bento:subprocess" {
            return ModuleLoadResponse::Sync(Ok(source(
                specifier,
                FastString::from_static(include_str!("subprocess.mjs")),
                ModuleType::JavaScript,
            )));
        }
        let specifier = specifier.clone();
        let node = self.node.clone();
        let permissions = self.permissions.clone();
        ModuleLoadResponse::Async(Box::pin(async move {
            let path = specifier.to_file_path().map_err(|_| {
                resolution_error(
                    specifier.as_str(),
                    specifier.as_str(),
                    "only local source files can be loaded",
                )
            })?;
            let checked = permissions
                .check_open(Cow::Borrowed(&path), OpenAccessKind::Read, Some("module import"))
                .map_err(JsErrorBox::from_err)?;
            refuse_transform(&path)?;
            let text = std::fs::read_to_string(checked).map_err(JsErrorBox::from_err)?;
            if path.extension().is_some_and(|extension| extension == "json") {
                return Ok(source(&specifier, text, ModuleType::Json));
            }
            let translated = node
                .translator
                .translate_cjs_to_esm(&specifier, Some(Cow::Owned(text)))
                .await
                .map_err(JsErrorBox::from_err)?;
            Ok(source(&specifier, translated.into_owned(), ModuleType::JavaScript))
        }))
    }

    fn get_host_defined_options<'s>(
        &self,
        scope: &mut deno_core::v8::PinScope<'s, '_>,
        _name: &str,
    ) -> Option<deno_core::v8::Local<'s, deno_core::v8::Data>> {
        Some(deno_node::create_host_defined_options(scope))
    }

    fn get_source_mapped_source_line(&self, name: &str, line: usize) -> Option<String> {
        let path = ModuleSpecifier::parse(name).ok()?.to_file_path().ok()?;
        std::fs::read_to_string(path).ok()?.split('\n').nth(line).map(ToOwned::to_owned)
    }
}

impl NodeRequireLoader for Loader {
    fn ensure_read_permission<'a>(
        &self,
        permissions: &mut PermissionsContainer,
        path: Cow<'a, Path>,
    ) -> Result<Cow<'a, Path>, JsErrorBox> {
        permissions
            .check_open(path, OpenAccessKind::Read, Some("require"))
            .map(|path| path.into_path())
            .map_err(JsErrorBox::from_err)
    }

    fn load_text_file_lossy(&self, path: &Path) -> Result<FastString, JsErrorBox> {
        refuse_transform(path)?;
        std::fs::read_to_string(path).map(Into::into).map_err(JsErrorBox::from_err)
    }

    fn is_maybe_cjs(&self, specifier: &ModuleSpecifier) -> Result<bool, PackageJsonLoadError> {
        self.node.tracker.is_maybe_cjs(specifier, MediaType::from_specifier(specifier))
    }

    fn is_maybe_cjs_from_require(
        &self,
        specifier: &ModuleSpecifier,
    ) -> Result<bool, PackageJsonLoadError> {
        self.node.tracker.is_maybe_cjs_from_require(specifier, MediaType::from_specifier(specifier))
    }
}

fn refuse_transform(path: &Path) -> Result<(), JsErrorBox> {
    if path.extension().is_some_and(|extension| {
        matches!(extension.to_str(), Some("ts" | "mts" | "cts" | "tsx" | "jsx"))
    }) {
        return Err(JsErrorBox::type_error(format!(
            "BENTO_TRANSFORM {} requires bento_compiler integration for TypeScript/JSX transformation",
            path.display()
        )));
    }
    Ok(())
}

fn source(
    specifier: &ModuleSpecifier,
    code: impl Into<FastString>,
    kind: ModuleType,
) -> ModuleSource {
    ModuleSource::new(kind, ModuleSourceCode::String(code.into()), specifier, None)
}

fn resolution_error(specifier: &str, referrer: &str, detail: &str) -> JsErrorBox {
    JsErrorBox::type_error(format!(
        "BENTO_RESOLUTION Cannot resolve {specifier:?} from {referrer}: {detail}. For missing installed packages, run bento install."
    ))
}
