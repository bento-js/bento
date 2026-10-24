use std::borrow::Cow;
use std::collections::BTreeSet;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use deno_core::{OpState, op2, url::Url};
use deno_error::JsErrorBox;
use node_resolver::{UrlOrPath, errors::ModuleNotFoundError};
use oxc_resolver::{ResolveContext, ResolveOptions, Resolver};
use parking_lot::Mutex;

#[derive(Clone, Debug)]
pub(crate) struct WatchInputs {
    root: PathBuf,
    paths: Arc<Mutex<BTreeSet<PathBuf>>>,
    sources: Arc<Mutex<BTreeSet<PathBuf>>>,
    require_resolver: Arc<Resolver>,
}

impl WatchInputs {
    pub(crate) fn new(root: PathBuf, paths: Arc<Mutex<BTreeSet<PathBuf>>>) -> Self {
        Self {
            root,
            paths,
            sources: Arc::default(),
            require_resolver: Arc::new(Resolver::new(ResolveOptions {
                condition_names: vec!["node".into(), "require".into(), "default".into()],
                extensions: [".js", ".cjs", ".mjs", ".wasm", ".json", ".node"]
                    .map(String::from)
                    .into(),
                node_path: false,
                builtin_modules: true,
                ..Default::default()
            })),
        }
    }

    pub(crate) fn record(&self, path: &Path) {
        let Some(canonical) = self.watch_path(path) else { return };
        if !self.sources.lock().insert(canonical.clone()) {
            return;
        }
        self.record_package_scopes(&canonical);
        self.paths.lock().insert(canonical);
    }

    pub(crate) fn record_failed_target(&self, path: &Path) {
        if let Some(canonical) = self.watch_path(path) {
            self.paths.lock().insert(canonical);
        }
    }

    pub(crate) fn record_failed_require(
        &self,
        request: &str,
        importer: &Path,
        extensions: Vec<String>,
    ) {
        if !self.is_source(importer) {
            return;
        }
        let Some(base) = importer.parent() else { return };
        let file_path = request
            .starts_with("file:")
            .then(|| Url::parse(request).ok().and_then(|url| url.to_file_path().ok()))
            .flatten();
        let request = file_path.as_ref().and_then(|path| path.to_str()).unwrap_or(request);
        let mut context = ResolveContext::default();
        let custom_resolver;
        let resolver = if self.require_resolver.options().extensions == extensions {
            &*self.require_resolver
        } else {
            let options = ResolveOptions { extensions, ..self.require_resolver.options().clone() };
            custom_resolver = self.require_resolver.clone_with_options(options);
            &custom_resolver
        };
        resolver.clear_cache();
        if resolver.resolve_with_context(base, request, None, &mut context).is_ok() {
            return;
        }
        for path in context.missing_dependencies {
            if path.file_name().is_none_or(|name| name != "package.json") {
                self.record_failed_target(&path);
            }
        }
        if request.starts_with("./")
            || request.starts_with("../")
            || Path::new(request).is_absolute()
        {
            self.record_failed_target(&base.join(request));
        }
    }

    pub(crate) fn record_missing_require_target(&self, target: &Path, importer: &Path) {
        if self.is_source(importer)
            && target.is_absolute()
            && std::fs::metadata(target)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        {
            self.record_failed_target(target);
        }
    }

    pub(crate) fn record_cjs_resolution_failure(&self, failure: &JsErrorBox) {
        let extension_probes =
            failure.get_inner_ref().is_some_and(|error| error.is::<ModuleNotFoundError>());
        let mut error: &(dyn Error + 'static) = failure;
        loop {
            if let Some(missing) = error.downcast_ref::<ModuleNotFoundError>() {
                let Some(importer) = missing.maybe_referrer.as_ref().and_then(local_path) else {
                    return;
                };
                if !self.is_source(&importer) {
                    return;
                }
                let Some(target) = local_path(&missing.specifier).filter(|path| path.is_absolute())
                else {
                    return;
                };
                self.record_failed_target(&target);
                if extension_probes && let Some(filename) = target.file_name() {
                    for extension in [".js", ".json"] {
                        let mut candidate = filename.to_owned();
                        candidate.push(extension);
                        self.record_failed_target(&target.with_file_name(candidate));
                    }
                }
                return;
            }
            let inner = error
                .downcast_ref::<JsErrorBox>()
                .and_then(JsErrorBox::get_inner_ref)
                .map(|inner| inner as &(dyn Error + 'static));
            let Some(source) = inner.or_else(|| error.source()) else { return };
            error = source;
        }
    }

    pub(crate) fn is_source(&self, path: &Path) -> bool {
        crate::sources::is_watch_source(&self.root, path)
    }

    fn watch_path(&self, path: &Path) -> Option<PathBuf> {
        if !self.is_source(path) {
            return None;
        }
        let canonical = bento_host::canonical_path(path).unwrap_or_else(|_| path.to_owned());
        self.is_source(&canonical).then_some(canonical)
    }

    fn record_package_scopes(&self, source: &Path) {
        let Some(parent) = source.parent() else { return };
        for directory in parent.ancestors() {
            if directory != self.root && self.root.starts_with(directory) {
                break;
            }
            let package = directory.join("package.json");
            let exists = package.exists();
            if exists || directory == parent || directory.starts_with(&self.root) {
                self.paths.lock().insert(package);
            }
            if exists {
                break;
            }
        }
    }
}

#[op2]
pub(crate) fn op_bento_failed_require(
    state: &OpState,
    #[string] request: &str,
    #[string] importer: &str,
    #[serde] extensions: Vec<String>,
) {
    state.borrow::<WatchInputs>().record_failed_require(request, Path::new(importer), extensions);
}

#[op2(fast)]
pub(crate) fn op_bento_missing_require_target(
    state: &OpState,
    #[string] target: &str,
    #[string] importer: &str,
) {
    state
        .borrow::<WatchInputs>()
        .record_missing_require_target(Path::new(target), Path::new(importer));
}

fn local_path(target: &UrlOrPath) -> Option<Cow<'_, Path>> {
    match target {
        UrlOrPath::Path(path) => Some(Cow::Borrowed(path)),
        UrlOrPath::Url(url) => url.to_file_path().ok().map(Cow::Owned),
    }
}
