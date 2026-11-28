use std::borrow::Cow;
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use deno_runtime::deno_permissions::{
    AllowDescriptor, AllowRunDescriptorParseResult, CheckedPath, DenyRunDescriptor, EnvDescriptor,
    EnvDescriptorParseError, FfiDescriptor, ImportDescriptor, NetDescriptor,
    NetDescriptorParseError, OpenAccessKind, PathCoverage, PathDescriptor, PathQueryDescriptor,
    PathResolveError, PermissionCheckError, PermissionDescriptorParser,
    Permissions as DenoPermissions, PermissionsContainer, ReadDescriptor, RunDescriptorParseError,
    RunQueryDescriptor, RuntimePermissionDescriptorParser, SpecialFilePathQueryDescriptor,
    SysDescriptor, SysDescriptorParseError, UnaryPermission, WriteDescriptor,
};
use parking_lot::Mutex;
use sys_traits::impls::RealSys;

use bento_host::{PathPattern, Permission, Permissions, Scope, canonical_path};
use bento_prompt::deno::Access;

/// The permissions one isolate holds: the main isolate's, or one worker's. As in Node, a
/// permission the guest drops is gone only from the isolate that dropped it.
pub(crate) struct Authority {
    permissions: Mutex<Permissions>,
    /// What Deno's operations check, rebuilt from `permissions` whenever they change. The
    /// isolate's operations hold clones that share the same checks.
    container: PermissionsContainer,
    model: Access,
    cwd: PathBuf,
    /// A read this isolate holds that the processes it starts do not inherit: a worker's
    /// reading beneath the working directory.
    own_read: Option<PathPattern>,
}

impl Authority {
    pub(crate) fn new(permissions: Permissions, cwd: PathBuf) -> Arc<Authority> {
        Authority::holding(permissions, cwd, None)
    }

    /// A worker's authority: the permissions the instance was launched with and, as in Node,
    /// reading everything beneath the working directory. A process the worker starts inherits
    /// only what the launch allowed, as a Node child process inherits only its flags.
    pub(crate) fn for_worker(launch: &Permissions, cwd: PathBuf) -> Arc<Authority> {
        let beneath = PathPattern::resolve("*", &cwd).ok().filter(|beneath| {
            launch.is_restricted()
                && match launch.allowed_paths(Permission::FsRead) {
                    Scope::All => false,
                    Scope::None => true,
                    Scope::Only(patterns) => !patterns.contains(beneath),
                }
        });
        let permissions = match &beneath {
            Some(beneath) => launch.clone().reading(beneath.clone()),
            None => launch.clone(),
        };
        Authority::holding(permissions, cwd, beneath)
    }

    fn holding(
        permissions: Permissions,
        cwd: PathBuf,
        own_read: Option<PathPattern>,
    ) -> Arc<Authority> {
        let parser = Arc::new(PathParser {
            inner: RuntimePermissionDescriptorParser::new(RealSys),
            cwd: cwd.clone(),
        });
        let container = PermissionsContainer::new(parser, deno_permissions(&permissions))
            .with_node_unix_sockets();
        let model = Access::new(permissions.holds(Permission::LanguageModel));
        let permissions = Mutex::new(permissions);
        Arc::new(Authority { permissions, container, model, cwd, own_read })
    }

    pub(crate) fn container(&self) -> &PermissionsContainer {
        &self.container
    }

    pub(crate) fn model(&self) -> &Access {
        &self.model
    }

    /// The permissions as they are now.
    pub(crate) fn current(&self) -> Permissions {
        self.permissions.lock().clone()
    }

    /// The permissions a process this isolate starts may hold at most: the current ones, less
    /// the read this isolate holds for itself alone.
    pub(crate) fn ceiling(&self) -> Permissions {
        let mut ceiling = self.current();
        if let Some(own_read) = &self.own_read {
            ceiling.drop_pattern(Permission::FsRead, own_read);
        }
        ceiling
    }

    pub(crate) fn holds(&self, permission: Permission) -> bool {
        self.permissions.lock().holds(permission)
    }

    /// The canonical location `reference`, a path relative to the guest's working directory,
    /// reaches; what file system checks compare.
    pub(crate) fn locate(&self, reference: &str) -> Option<PathBuf> {
        canonical_path(&self.cwd.join(reference)).ok()
    }

    /// The pattern `reference` names, resolved as the command line resolves a flag value
    /// against the guest's working directory.
    pub(crate) fn pattern(&self, reference: &str) -> Option<PathPattern> {
        PathPattern::resolve(reference, &self.cwd).ok()
    }

    /// Takes away part of this isolate's permissions for good.
    pub(crate) fn give_up(&self, change: impl FnOnce(&mut Permissions)) {
        let mut permissions = self.permissions.lock();
        change(&mut permissions);
        if !permissions.holds_any(Permission::LanguageModel) {
            self.model.revoke();
        }
        self.container.set_permissions(deno_permissions(&permissions));
    }
}

/// Deno's form of `permissions`. Files keep Node's matching; network, child processes, and
/// addons are all or nothing. Bento's own operations read and write the environment, so the
/// environment entry only answers `Deno.permissions.query()`. System information is always
/// available, as in Node. Remote modules never load whatever Deno allows, because Bento's
/// loader refuses them.
fn deno_permissions(permissions: &Permissions) -> DenoPermissions {
    let mut deno = DenoPermissions::none_without_prompt();
    deno.read = paths(permissions, Permission::FsRead, PathDescriptor::into_read);
    deno.write = paths(permissions, Permission::FsWrite, PathDescriptor::into_write);
    deno.net = whole(permissions.holds(Permission::Net));
    deno.env = DenoPermissions::new_unary(
        listed(permissions.allowed_variables(), |pattern| {
            EnvDescriptor::new(Cow::Owned(pattern.to_string()))
        }),
        listed(permissions.denied_variables(), |pattern| {
            EnvDescriptor::new(Cow::Owned(pattern.to_string()))
        }),
        false,
    );
    deno.sys = system(permissions.holds(Permission::Inspector));
    deno.run = whole(permissions.holds(Permission::ChildProcess));
    deno.ffi = whole(permissions.holds(Permission::Addons));
    deno.import = UnaryPermission::<ImportDescriptor>::allow_all();
    deno
}

fn whole<T: AllowDescriptor>(held: bool) -> UnaryPermission<T> {
    if held { UnaryPermission::allow_all() } else { DenoPermissions::new_unary(None, None, false) }
}

/// Every system query, as in Node. Deno files opening and connecting to the inspector under
/// the system query `inspector`, so that one follows Node's inspector permission.
fn system(inspector: bool) -> UnaryPermission<SysDescriptor> {
    if inspector {
        return UnaryPermission::allow_all();
    }
    let inspector =
        SysDescriptor::parse("inspector".to_owned()).expect("a system query Deno knows");
    DenoPermissions::new_unary(Some(Vec::new()), Some(vec![inspector]), false)
}

/// Deno's list for `scope`: none for nothing, and an empty list, which Deno reads as
/// everything, for all of it.
fn listed<P: Ord, T>(scope: &Scope<P>, descriptor: impl Fn(&P) -> T) -> Option<Vec<T>> {
    match scope {
        Scope::None => None,
        Scope::All => Some(Vec::new()),
        Scope::Only(patterns) if patterns.is_empty() => None,
        Scope::Only(patterns) => Some(patterns.iter().map(descriptor).collect()),
    }
}

fn paths<T: AllowDescriptor<DenyDesc = T>>(
    permissions: &Permissions,
    permission: Permission,
    descriptor: fn(PathDescriptor) -> T,
) -> UnaryPermission<T> {
    let list = |scope| listed(scope, |pattern| descriptor(path_descriptor(pattern)));
    DenoPermissions::new_unary(
        list(permissions.allowed_paths(permission)),
        list(permissions.denied_paths(permission)),
        false,
    )
}

fn path_descriptor(pattern: &PathPattern) -> PathDescriptor {
    match pattern {
        PathPattern::Exact(path) => {
            PathDescriptor::new_with_coverage(path.clone(), PathCoverage::Exact)
        }
        PathPattern::Prefix { prefix, .. } => {
            PathDescriptor::new_with_coverage(prefix.clone(), PathCoverage::Prefix)
        }
    }
}

// The upstream parser has no access-kind argument; set this only during a no-follow check.
thread_local! {
    static NO_FOLLOW: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn check_no_follow<'a>(
    permissions: &PermissionsContainer,
    path: Cow<'a, Path>,
    kind: OpenAccessKind,
    api: &str,
) -> Result<CheckedPath<'a>, PermissionCheckError> {
    with_no_follow(|| permissions.check_open(path, kind, Some(api)))
}

pub(crate) fn check_remove_no_follow<'a>(
    permissions: &PermissionsContainer,
    path: Cow<'a, Path>,
    recursive: bool,
) -> Result<CheckedPath<'a>, PermissionCheckError> {
    with_no_follow(|| {
        if recursive {
            permissions.check_write(path, "node:fs")
        } else {
            permissions.check_write_partial(path, "node:fs")
        }
    })
}

fn with_no_follow<T>(check: impl FnOnce() -> T) -> T {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            NO_FOLLOW.set(self.0);
        }
    }
    let _reset = Reset(NO_FOLLOW.replace(true));
    check()
}

#[derive(Debug)]
struct PathParser {
    inner: RuntimePermissionDescriptorParser<RealSys>,
    cwd: PathBuf,
}

macro_rules! delegate {
    ($($method:ident -> $result:ty;)*) => {$ (
        fn $method(&self, text: &str) -> $result { self.inner.$method(text) }
    )*};
}

impl PermissionDescriptorParser for PathParser {
    delegate! {
        parse_read_descriptor -> Result<ReadDescriptor, PathResolveError>;
        parse_write_descriptor -> Result<WriteDescriptor, PathResolveError>;
        parse_net_descriptor -> Result<NetDescriptor, NetDescriptorParseError>;
        parse_import_descriptor -> Result<ImportDescriptor, NetDescriptorParseError>;
        parse_env_descriptor -> Result<EnvDescriptor, EnvDescriptorParseError>;
        parse_sys_descriptor -> Result<SysDescriptor, SysDescriptorParseError>;
        parse_allow_run_descriptor -> Result<AllowRunDescriptorParseResult, RunDescriptorParseError>;
        parse_deny_run_descriptor -> Result<DenyRunDescriptor, PathResolveError>;
        parse_ffi_descriptor -> Result<FfiDescriptor, PathResolveError>;
        parse_net_query -> Result<NetDescriptor, NetDescriptorParseError>;
    }

    /// Checks compare the location a path reaches, but a denial names the path as the
    /// operation was given it, as Node's `resource` does.
    fn parse_path_query<'a>(
        &self,
        path: Cow<'a, Path>,
    ) -> Result<PathQueryDescriptor<'a>, PathResolveError> {
        let requested = path.to_string_lossy().into_owned();
        let absolute = if path.is_absolute() { path.into_owned() } else { self.cwd.join(path) };
        let bytes = absolute.as_os_str().as_encoded_bytes();
        let directory_suffix = bytes.ends_with(b"/") || bytes.ends_with(b"/.");
        let canonical = if NO_FOLLOW.get() && !directory_suffix && absolute.file_name().is_some() {
            let parent = canonical_path(absolute.parent().expect("absolute file has a parent"))
                .map_err(PathResolveError::CwdResolve)?;
            parent.join(absolute.file_name().expect("file name exists"))
        } else {
            canonical_path(&absolute).map_err(PathResolveError::CwdResolve)?
        };
        Ok(self.inner.parse_path_query(Cow::Owned(canonical))?.with_requested(requested))
    }

    fn parse_special_file_descriptor<'a>(
        &self,
        path: PathQueryDescriptor<'a>,
    ) -> Result<SpecialFilePathQueryDescriptor<'a>, PathResolveError> {
        self.inner.parse_special_file_descriptor(path)
    }

    fn parse_run_query<'a>(
        &self,
        requested: &'a str,
    ) -> Result<RunQueryDescriptor<'a>, RunDescriptorParseError> {
        self.inner.parse_run_query(requested)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use bento_host::{PermissionSet, Permissions, Scope};
    use deno_runtime::deno_permissions::{OpenAccessKind, PermissionsContainer};

    use super::{Authority, Cow};

    fn reads(container: &PermissionsContainer, path: &str) -> bool {
        container.check_open(Cow::Borrowed(Path::new(path)), OpenAccessKind::Read, None).is_ok()
    }

    #[test]
    fn an_empty_list_of_paths_allows_nothing_rather_than_everything() {
        let allowed =
            PermissionSet { fs_read: Scope::Only(BTreeSet::new()), ..PermissionSet::default() };
        let authority = Authority::new(Permissions::restricted(allowed), "/".into());

        assert!(!reads(authority.container(), "/etc/hosts"));
    }

    #[test]
    fn an_unrestricted_instance_denies_only_what_it_was_told_to() {
        let denied = PermissionSet {
            fs_read: Scope::only([bento_host::PathPattern::Exact("/bin/sh".into())]),
            ..PermissionSet::default()
        };
        let authority = Authority::new(Permissions::unrestricted_denying(denied), "/".into());

        assert!(!reads(authority.container(), "/bin/sh"));
        assert!(reads(authority.container(), "/bin/ls"));
    }
}
