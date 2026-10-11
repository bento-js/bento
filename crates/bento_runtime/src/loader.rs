use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashSet;
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
use oxc_resolver::{ResolveContext, ResolveOptions, Resolver};

use crate::host::HostModules;
use crate::lifecycle::Control;
use crate::node::NodeServices;
use crate::sources::Sources;

#[derive(Clone)]
pub(crate) struct LoaderFactory {
    pub node: NodeServices,
    pub cwd: PathBuf,
    pub modules: HostModules,
    pub control: Arc<Control>,
    pub sources: Arc<Sources>,
}

impl LoaderFactory {
    fn create(&self, permissions: PermissionsContainer) -> CreateModuleLoaderResult {
        let loader = Rc::new(Loader {
            node: self.node.clone(),
            permissions,
            cwd: self.cwd.clone(),
            modules: self.modules.clone(),
            sources: self.sources.clone(),
            application_resolutions: RefCell::default(),
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
    modules: HostModules,
    sources: Arc<Sources>,
    application_resolutions: RefCell<HashSet<ModuleSpecifier>>,
}

impl ModuleLoader for Loader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        kind: ResolutionKind,
    ) -> Result<ModuleSpecifier, JsErrorBox> {
        if matches!(specifier, "bento:compat" | "bento:subprocess") {
            return ModuleSpecifier::parse(specifier).map_err(JsErrorBox::from_err);
        }
        if specifier.starts_with("host:") {
            if !self.modules.contains(specifier) {
                return Err(self.sources.resolution_error(
                    specifier,
                    referrer,
                    "the host did not register this module in this instance",
                ));
            }
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
        let importer =
            ModuleSpecifier::parse(referrer).ok().and_then(|url| url.to_file_path().ok());
        let watch_failure =
            importer.as_deref().is_some_and(|path| self.node.inputs.is_source(path));
        if let Ok(url) = ModuleSpecifier::parse(specifier) {
            if url.scheme() != "file" {
                return Err(self.sources.resolution_error(
                    specifier,
                    referrer,
                    "remote and unregistered module schemes are unavailable",
                ));
            }
            if watch_failure
                || kind == ResolutionKind::MainModule
                    && url.to_file_path().is_ok_and(|path| self.node.inputs.is_source(&path))
            {
                self.application_resolutions.borrow_mut().insert(url.clone());
            }
            return Ok(url);
        }
        let base = importer.as_deref().and_then(Path::parent).unwrap_or(&self.cwd);
        let mut context = ResolveContext::default();
        let resolved = if watch_failure {
            self.resolver.resolve_with_context(base, specifier, None, &mut context)
        } else {
            self.resolver.resolve(base, specifier)
        }
        .map_err(|error| {
            if watch_failure {
                for path in context.missing_dependencies {
                    if path.file_name().is_none_or(|name| name != "package.json") {
                        self.node.inputs.record_failed_target(&path);
                    }
                }
                if specifier.starts_with("./")
                    || specifier.starts_with("../")
                    || Path::new(specifier).is_absolute()
                {
                    self.node.inputs.record_failed_target(&base.join(specifier));
                }
            }
            self.sources.resolution_error(specifier, referrer, &error.to_string())
        })?;
        let url = ModuleSpecifier::from_file_path(resolved.path()).map_err(|_| {
            self.sources.resolution_error(
                specifier,
                referrer,
                "resolved path is not an absolute file",
            )
        })?;
        if watch_failure {
            self.application_resolutions.borrow_mut().insert(url.clone());
        }
        Ok(url)
    }

    fn load(
        &self,
        specifier: &ModuleSpecifier,
        referrer: Option<&ModuleLoadReferrer>,
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
        if let Some(code) = self.modules.source(specifier.as_str()) {
            return ModuleLoadResponse::Sync(Ok(source(specifier, code, ModuleType::JavaScript)));
        }
        let specifier = specifier.clone();
        let node = self.node.clone();
        let permissions = self.permissions.clone();
        let sources = self.sources.clone();
        let watch_failure = self.application_resolutions.borrow().contains(&specifier)
            || referrer
                .and_then(|referrer| referrer.specifier.to_file_path().ok())
                .is_some_and(|path| self.node.inputs.is_source(&path));
        ModuleLoadResponse::Async(Box::pin(async move {
            let path = specifier.to_file_path().map_err(|_| {
                sources.resolution_error(
                    specifier.as_str(),
                    specifier.as_str(),
                    "only local source files can be loaded",
                )
            })?;
            let checked = permissions
                .check_open(Cow::Borrowed(&path), OpenAccessKind::Read, Some("module import"))
                .map_err(JsErrorBox::from_err)?;
            let text = match std::fs::read_to_string(checked) {
                Ok(text) => {
                    node.inputs.record(&path);
                    text
                }
                Err(error) => {
                    if watch_failure {
                        node.inputs.record_failed_target(&path);
                    }
                    return Err(JsErrorBox::from_err(error));
                }
            };
            if path.extension().is_some_and(|extension| extension == "json") {
                return Ok(source(&specifier, text, ModuleType::Json));
            }
            let text = sources.transform(&path, text)?;
            let translated = node
                .translator
                .translate_cjs_to_esm(&specifier, Some(Cow::Owned(text)))
                .await
                .map_err(|error| {
                    if let node_resolver::analyze::TranslateCjsToEsmError::ExportAnalysis(failure) =
                        &error
                    {
                        node.inputs.record_cjs_resolution_failure(failure);
                    }
                    JsErrorBox::from_err(error)
                })?;
            Ok(source(&specifier, translated.into_owned(), ModuleType::JavaScript))
        }))
    }

    fn get_source_map(&self, name: &str) -> Option<Cow<'_, [u8]>> {
        self.sources.source_map(name).map(Cow::Owned)
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
        self.node.inputs.record(path);
        let text = std::fs::read_to_string(path).map_err(JsErrorBox::from_err)?;
        Ok(self.sources.transform(path, text)?.into())
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

fn source(
    specifier: &ModuleSpecifier,
    code: impl Into<FastString>,
    kind: ModuleType,
) -> ModuleSource {
    ModuleSource::new(kind, ModuleSourceCode::String(code.into()), specifier, None)
}
