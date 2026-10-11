use std::borrow::Cow;
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use deno_runtime::deno_permissions::*;
use sys_traits::impls::RealSys;

use bento_host::{Grants, Scope, canonical_path};

pub(crate) fn container(grants: &Grants, cwd: PathBuf) -> anyhow::Result<PermissionsContainer> {
    let parser =
        Arc::new(PathParser { inner: RuntimePermissionDescriptorParser::new(RealSys), cwd });
    let options = PermissionsOptions {
        allow_read: strings(&grants.read, |path| path.to_string_lossy().into_owned()),
        allow_write: strings(&grants.write, |path| path.to_string_lossy().into_owned()),
        allow_net: strings(&grants.net, ToString::to_string),
        allow_env: strings(&grants.env, Clone::clone),
        allow_ffi: strings(&grants.addons, |path| path.to_string_lossy().into_owned()),
        // Node's fs.access implementation needs the current identity to evaluate mode bits.
        allow_sys: Some(vec!["uid".into(), "gid".into()]),
        prompt: false,
        ..Default::default()
    };
    let permissions = Permissions::from_options(parser.as_ref(), &options)?;
    Ok(PermissionsContainer::new(parser, permissions))
}

fn strings<T: Ord>(scope: &Scope<T>, stringify: impl Fn(&T) -> String) -> Option<Vec<String>> {
    match scope {
        Scope::None => None,
        // Upstream interprets Some([]) as unrestricted authority, not an empty allow-list.
        Scope::All => Some(Vec::new()),
        Scope::Only(items) if items.is_empty() => None,
        Scope::Only(items) => Some(items.iter().map(stringify).collect()),
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

    fn parse_path_query<'a>(
        &self,
        path: Cow<'a, Path>,
    ) -> Result<PathQueryDescriptor<'a>, PathResolveError> {
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
        self.inner.parse_path_query(Cow::Owned(canonical))
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

    use super::strings;
    use bento_host::Scope;

    #[test]
    fn raw_empty_scopes_convert_like_no_authority_not_all_authority() {
        let empty = Scope::<String>::Only(BTreeSet::new());
        assert_eq!(strings(&empty, Clone::clone), strings(&Scope::None, Clone::clone));
        assert_eq!(strings(&Scope::<String>::All, Clone::clone), Some(Vec::new()));
        assert_eq!(
            strings(&Scope::only(["allowed".to_owned()]), Clone::clone),
            Some(vec!["allowed".to_owned()])
        );
    }
}
