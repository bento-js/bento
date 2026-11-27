use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use deno_core::{OpDecl, OpState, op2};
use deno_error::JsErrorBox;
use deno_node::ops::require::{RequireError, RequireErrorKind};
use deno_runtime::deno_permissions::{OpenAccessKind, PermissionsContainer};
use node_resolver::cache::NodeResolutionThreadLocalCache;
use node_resolver::{NodeResolutionKind, PackageJsonResolver, ResolutionMode, UrlOrPathRef};
use sys_traits::impls::RealSys;

use crate::environment::InstanceEnvironment;
use crate::node::Resolver;

/// deno_node operations whose permission checks Node makes elsewhere, or not at all.
pub(crate) fn replacement(name: &str) -> Option<OpDecl> {
    Some(match name {
        "op_require_stat" => bento_require_stat(),
        "op_require_resolve_exports" => bento_require_resolve_exports(),
        "op_worker_threads_filename" => bento_worker_threads_filename(),
        _ => return None,
    })
}

/// deno_node 0.197.0's `op_worker_threads_filename` without its read checks. Node reads a
/// worker's script in the worker, so a script the instance may not read fails there and reaches
/// the `Worker`'s `'error'` event rather than throwing from `new Worker()`.
#[op2]
#[string]
fn bento_worker_threads_filename(
    state: &OpState,
    #[string] specifier: &str,
) -> Result<Option<String>, JsErrorBox> {
    if specifier.starts_with("data:") {
        return Ok(None);
    }
    if specifier.starts_with("file:") {
        return Ok(Some(
            deno_core::ModuleSpecifier::parse(specifier).map_err(JsErrorBox::from_err)?.into(),
        ));
    }
    if Path::new(specifier).is_relative() && !specifier.starts_with('.') {
        return Err(JsErrorBox::generic("Relative path entries must start with '.' or '..'"));
    }
    let cwd = &state.borrow::<InstanceEnvironment>().cwd;
    let path = deno_path_util::normalize_path(Cow::Owned(cwd.join(specifier)));
    let path = std::fs::canonicalize(&path).map_or(path, Cow::Owned);
    let url = deno_path_util::url_from_file_path(&path).map_err(JsErrorBox::from_err)?;
    Ok(Some(url.into()))
}

/// Node's `internalModuleStat`, which `require()` uses to probe the directories and candidate
/// files of a lookup: 1 for a directory, 0 for anything else, or the negated `errno`. Node does
/// not check the permission model here; it refuses the realpath or read that follows, so the
/// denial names that file rather than a directory the lookup passed through.
#[op2(fast)]
fn bento_require_stat(#[string] path: &str) -> i32 {
    match std::fs::metadata(path) {
        Ok(metadata) => i32::from(metadata.is_dir()),
        Err(error) => error.raw_os_error().map_or(-1, |code| -code),
    }
}

/// deno_node 0.197.0's `op_require_resolve_exports`, which `require()` calls for a bare request
/// in each `node_modules` directory of its lookup, after the check Node makes there: Node reads
/// `<node_modules>/<name>/package.json` under the permission model, whether or not the package
/// exists, before the manifest's `exports` can shape the result. deno_node reads it unchecked.
#[op2(stack_trace)]
#[string]
fn bento_require_resolve_exports(
    state: &mut OpState,
    uses_local_node_modules_dir: bool,
    #[string] modules_path: &str,
    #[string] _request: &str,
    #[string] name: &str,
    #[string] expansion: &str,
    #[string] parent_path: &str,
) -> Result<Option<String>, RequireError> {
    let modules_path = Path::new(modules_path);
    let package_dir = deno_path_util::normalize_path(Cow::Owned(modules_path.join(name)));
    state
        .borrow::<PermissionsContainer>()
        .check_open(
            Cow::Owned(package_dir.join("package.json")),
            OpenAccessKind::Read,
            Some("require"),
        )
        .map_err(|error| RequireErrorKind::Permission(JsErrorBox::from_err(error)).into_box())?;
    let resolver = state.borrow::<Arc<Resolver>>();
    let modules_url = deno_path_util::url_from_file_path(modules_path)?;
    let manifest_dir = if resolver.in_npm_package(&modules_url) && !uses_local_node_modules_dir
        || !package_dir.is_dir()
    {
        modules_path
    } else {
        package_dir.as_ref()
    };
    let Some(manifest) = state
        .borrow::<Arc<PackageJsonResolver<RealSys>>>()
        .load_package_json(&manifest_dir.join("package.json"))?
    else {
        return Ok(None);
    };
    let Some(exports) = &manifest.exports else {
        return Ok(None);
    };
    let referrer = (!parent_path.is_empty()).then(|| PathBuf::from(parent_path));
    NodeResolutionThreadLocalCache::clear();
    let resolved = resolver.package_exports_resolve(
        &manifest.path,
        &format!(".{expansion}"),
        exports,
        referrer.as_deref().map(UrlOrPathRef::from_path).as_ref(),
        ResolutionMode::Require,
        resolver.require_conditions(),
        NodeResolutionKind::Execution,
    )?;
    Ok(Some(if resolved.is_file() {
        resolved.into_path()?.to_string_lossy().into_owned()
    } else {
        resolved.to_string_lossy().into_owned()
    }))
}
