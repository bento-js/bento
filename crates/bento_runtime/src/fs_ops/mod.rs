mod directories;
mod metadata;
mod mutations;
mod open;
mod read;
mod stat;
#[cfg(test)]
mod tests;

use std::borrow::Cow;
use std::path::Path;

use deno_core::{OpDecl, OpState};
use deno_runtime::deno_fs::{FileSystemRc, FsOpsError};
use deno_runtime::deno_permissions::{CheckedPath, OpenAccessKind, PermissionsContainer};

pub(crate) fn replacement(name: &str) -> Option<OpDecl> {
    Some(match name {
        "op_fs_open_sync" => open::bento_open_sync(),
        "op_fs_open_async" => open::bento_open_async(),
        "op_node_open_sync" => open::bento_node_open_sync(),
        "op_node_open" => open::bento_node_open_async(),
        "op_fs_mkdir_sync" => directories::bento_mkdir_sync(),
        "op_fs_mkdir_async" => directories::bento_mkdir_async(),
        "op_node_rmdir_sync" => directories::bento_rmdir_sync(),
        "op_node_rmdir" => directories::bento_rmdir_async(),
        "op_fs_lstat_sync" => read::bento_lstat_sync(),
        "op_fs_lstat_async" => read::bento_lstat_async(),
        "op_fs_read_link_sync" => read::bento_read_link_sync(),
        "op_fs_read_link_async" => read::bento_read_link_async(),
        "op_fs_remove_sync" => mutations::bento_remove_sync(),
        "op_fs_remove_async" => mutations::bento_remove_async(),
        "op_fs_rename_sync" => mutations::bento_rename_sync(),
        "op_fs_rename_async" => mutations::bento_rename_async(),
        "op_fs_symlink_sync" => mutations::bento_symlink_sync(),
        "op_fs_symlink_async" => mutations::bento_symlink_async(),
        "op_fs_link_sync" => mutations::bento_link_sync(),
        "op_fs_link_async" => mutations::bento_link_async(),
        "op_node_lutimes_sync" => metadata::bento_lutimes_sync(),
        "op_node_lutimes" => metadata::bento_lutimes_async(),
        "op_node_lchown_sync" => metadata::bento_lchown_sync(),
        "op_node_lchown" => metadata::bento_lchown_async(),
        "op_node_lchmod_sync" => metadata::bento_lchmod_sync(),
        "op_node_lchmod" => metadata::bento_lchmod_async(),
        _ => return None,
    })
}

fn checked<'a>(
    state: &OpState,
    path: Cow<'a, Path>,
    kind: OpenAccessKind,
) -> Result<CheckedPath<'a>, FsOpsError> {
    let requested = requested_path(state, path);
    crate::permissions::check_no_follow(
        state.borrow::<PermissionsContainer>(),
        Cow::Borrowed(&requested),
        kind,
        "node:fs",
    )?;
    // Checking the resolved location must not change the native operation's path semantics.
    Ok(CheckedPath::unsafe_new(requested))
}

fn checked_remove<'a>(
    state: &OpState,
    path: Cow<'a, Path>,
    recursive: bool,
) -> Result<CheckedPath<'a>, FsOpsError> {
    let requested = requested_path(state, path);
    crate::permissions::check_remove_no_follow(
        state.borrow::<PermissionsContainer>(),
        Cow::Borrowed(&requested),
        recursive,
    )?;
    Ok(CheckedPath::unsafe_new(requested))
}

fn requested_path<'a>(state: &OpState, path: Cow<'a, Path>) -> Cow<'a, Path> {
    if path.is_absolute() || path.as_os_str().is_empty() {
        path
    } else {
        Cow::Owned(state.borrow::<crate::ops::InstanceEnvironment>().cwd.join(path))
    }
}

fn fs(state: &OpState) -> &FileSystemRc {
    state.borrow::<FileSystemRc>()
}
