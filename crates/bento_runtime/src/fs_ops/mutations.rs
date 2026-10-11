use std::borrow::Cow;
use std::cell::RefCell;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use deno_core::{OpState, op2};
use deno_runtime::deno_fs::{FsError, FsFileType, FsOpsError, FsOpsErrorKind};
use deno_runtime::deno_permissions::{
    CheckedPath, CheckedPathBuf, OpenAccessKind, PermissionCheckError, PermissionsContainer,
};

use super::{checked, checked_remove, fs};

fn has_link_with_directory_suffix(path: &Path) -> Result<bool, FsError> {
    let bytes = path.as_os_str().as_bytes();
    if !bytes.ends_with(b"/") {
        return Ok(false);
    }
    let Some(end) = bytes.iter().rposition(|byte| *byte != b'/') else {
        return Ok(false);
    };
    let leaf = Path::new(OsStr::from_bytes(&bytes[..=end]));
    Ok(std::fs::symlink_metadata(leaf)?.file_type().is_symlink())
}

#[op2(fast, stack_trace)]
pub(super) fn bento_remove_sync(
    state: &mut OpState,
    #[string] path: &str,
    recursive: bool,
) -> Result<(), FsOpsError> {
    let path = checked_remove(state, Cow::Borrowed(Path::new(path)), recursive)?;
    // A trailing slash makes lstat follow the link; it must not turn unlink into rmdir.
    if has_link_with_directory_suffix(&path)? {
        std::fs::remove_file(&path).map_err(FsError::Io)?;
    } else {
        fs(state).remove_sync(&path, recursive)?;
    }
    Ok(())
}

#[op2(stack_trace)]
pub(super) async fn bento_remove_async(
    state: Rc<RefCell<OpState>>,
    #[string] path: String,
    recursive: bool,
) -> Result<(), FsOpsError> {
    let (fs, path) = {
        let state = state.borrow();
        let path = checked_remove(&state, Cow::Owned(PathBuf::from(path)), recursive)?;
        (fs(&state).clone(), path.into_owned())
    };
    if has_link_with_directory_suffix(&path)? {
        tokio::task::spawn_blocking(move || std::fs::remove_file(path))
            .await
            .map_err(FsError::JoinError)?
            .map_err(FsError::Io)?;
    } else {
        fs.remove_async(path, recursive).await?;
    }
    Ok(())
}

#[op2(fast, stack_trace)]
pub(super) fn bento_rename_sync(
    state: &mut OpState,
    #[string] from: &str,
    #[string] to: &str,
) -> Result<(), FsOpsError> {
    let from = checked(state, Cow::Borrowed(Path::new(from)), OpenAccessKind::ReadWriteNoFollow)?;
    let to = checked(state, Cow::Borrowed(Path::new(to)), OpenAccessKind::WriteNoFollow)?;
    fs(state).rename_sync(&from, &to)?;
    Ok(())
}

#[op2(stack_trace)]
pub(super) async fn bento_rename_async(
    state: Rc<RefCell<OpState>>,
    #[string] from: String,
    #[string] to: String,
) -> Result<(), FsOpsError> {
    let (fs, from, to) = {
        let state = state.borrow();
        let from =
            checked(&state, Cow::Owned(PathBuf::from(from)), OpenAccessKind::ReadWriteNoFollow)?;
        let to = checked(&state, Cow::Owned(PathBuf::from(to)), OpenAccessKind::WriteNoFollow)?;
        (fs(&state).clone(), from.into_owned(), to.into_owned())
    };
    fs.rename_async(from, to).await?;
    Ok(())
}

fn check_symlink_permissions(state: &OpState, api: &str) -> Result<(), FsOpsError> {
    let permissions = state.borrow::<PermissionsContainer>();
    // Targets resolve at use time; scoped creation permits directory-swap escapes.
    permissions.check_write_all(api).and_then(|()| permissions.check_read_all(api)).map_err(
        |error| {
            let error = match error {
                PermissionCheckError::PermissionDenied(mut denied) => {
                    denied.custom_message = Some(format!(
                        "{api} requires unscoped read and write grants; symlink creation cannot use path-scoped grants"
                    ));
                    PermissionCheckError::PermissionDenied(denied)
                }
                other => other,
            };
            FsOpsErrorKind::Permission(error).into_box()
        },
    )
}

#[op2(stack_trace)]
pub(super) fn bento_symlink_sync(
    state: &mut OpState,
    #[string] target: &str,
    #[string] link: &str,
    #[serde] file_type: Option<FsFileType>,
) -> Result<(), FsOpsError> {
    check_symlink_permissions(state, "Deno.symlinkSync()")?;
    let link = checked(state, Cow::Borrowed(Path::new(link)), OpenAccessKind::WriteNoFollow)?;
    let target = CheckedPath::unsafe_new(Cow::Borrowed(Path::new(target)));
    fs(state).symlink_sync(&target, &link, file_type)?;
    Ok(())
}

#[op2(stack_trace)]
pub(super) async fn bento_symlink_async(
    state: Rc<RefCell<OpState>>,
    #[string] target: String,
    #[string] link: String,
    #[serde] file_type: Option<FsFileType>,
) -> Result<(), FsOpsError> {
    let (fs, link) = {
        let state = state.borrow();
        check_symlink_permissions(&state, "Deno.symlink()")?;
        let link = checked(&state, Cow::Owned(PathBuf::from(link)), OpenAccessKind::WriteNoFollow)?;
        (fs(&state).clone(), link.into_owned())
    };
    let target = CheckedPathBuf::unsafe_new(PathBuf::from(target));
    fs.symlink_async(target, link, file_type).await?;
    Ok(())
}

#[op2(fast, stack_trace)]
pub(super) fn bento_link_sync(
    state: &mut OpState,
    #[string] from: &str,
    #[string] to: &str,
) -> Result<(), FsOpsError> {
    let from = checked(state, Cow::Borrowed(Path::new(from)), OpenAccessKind::ReadWriteNoFollow)?;
    let to = checked(state, Cow::Borrowed(Path::new(to)), OpenAccessKind::WriteNoFollow)?;
    fs(state).link_sync(&from, &to)?;
    Ok(())
}

#[op2(stack_trace)]
pub(super) async fn bento_link_async(
    state: Rc<RefCell<OpState>>,
    #[string] from: String,
    #[string] to: String,
) -> Result<(), FsOpsError> {
    let (fs, from, to) = {
        let state = state.borrow();
        let from =
            checked(&state, Cow::Owned(PathBuf::from(from)), OpenAccessKind::ReadWriteNoFollow)?;
        let to = checked(&state, Cow::Owned(PathBuf::from(to)), OpenAccessKind::WriteNoFollow)?;
        (fs(&state).clone(), from.into_owned(), to.into_owned())
    };
    fs.link_async(from, to).await?;
    Ok(())
}
