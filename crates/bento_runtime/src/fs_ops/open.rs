use std::borrow::Cow;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use deno_core::{OpState, ResourceId, op2};
use deno_runtime::deno_fs::{FsOpsError, OpenOptions};
use deno_runtime::deno_io::{FdTable, fs::FileResource};
use deno_runtime::deno_permissions::{CheckedPath, OpenAccessKind, PermissionsContainer};

use super::{checked, fs};

fn checked_open<'a>(
    state: &OpState,
    path: Cow<'a, Path>,
    options: OpenOptions,
) -> Result<CheckedPath<'a>, FsOpsError> {
    let access = options.access_kind();
    let no_follow = options.create_new
        || options.custom_flags.is_some_and(|flags| flags & libc::O_NOFOLLOW != 0);
    if !no_follow {
        return state
            .borrow::<PermissionsContainer>()
            .check_open(path, access, Some("node:fs.open"))
            .map_err(Into::into);
    }
    let access = match (access.is_read(), access.is_write()) {
        (true, true) => OpenAccessKind::ReadWriteNoFollow,
        (false, true) => OpenAccessKind::WriteNoFollow,
        _ => OpenAccessKind::ReadNoFollow,
    };
    checked(state, path, access)
}

fn node_options(flags: i32, mode: u32) -> OpenOptions {
    OpenOptions { mode: Some(mode), ..OpenOptions::from(flags) }
}

#[op2(fast, stack_trace)]
#[smi]
pub(super) fn bento_node_open_sync(
    state: &mut OpState,
    #[string] path: &str,
    #[smi] flags: i32,
    #[smi] mode: u32,
) -> Result<i32, FsOpsError> {
    let options = node_options(flags, mode);
    let path = checked_open(state, Cow::Borrowed(Path::new(path)), options)?;
    let file = fs(state).open_sync(&path, options)?;
    let fd = file.clone().backing_fd().ok_or(deno_runtime::deno_fs::FsError::NotSupported)?;
    state.borrow_mut::<FdTable>().register(fd, file);
    Ok(fd)
}

#[op2(stack_trace)]
#[smi]
pub(super) async fn bento_node_open_async(
    state: Rc<RefCell<OpState>>,
    #[string] path: String,
    #[smi] flags: i32,
    #[smi] mode: u32,
) -> Result<i32, FsOpsError> {
    let options = node_options(flags, mode);
    let (fs, path) = {
        let state = state.borrow();
        let path = checked_open(&state, Cow::Owned(PathBuf::from(path)), options)?;
        (fs(&state).clone(), path.into_owned())
    };
    let file = fs.open_async(path, options).await?;
    let fd = file.clone().backing_fd().ok_or(deno_runtime::deno_fs::FsError::NotSupported)?;
    state.borrow_mut().borrow_mut::<FdTable>().register(fd, file);
    Ok(fd)
}

#[op2(stack_trace)]
#[smi]
pub(super) fn bento_open_sync(
    state: &mut OpState,
    #[string] path: &str,
    #[scoped] options: Option<OpenOptions>,
) -> Result<ResourceId, FsOpsError> {
    let options = options.unwrap_or_else(OpenOptions::read);
    let path = checked_open(state, Cow::Borrowed(Path::new(path)), options)?;
    let file = fs(state).open_sync(&path, options)?;
    Ok(state.resource_table.add(FileResource::new(file, "fsFile".to_owned())))
}

#[op2(stack_trace)]
#[smi]
pub(super) async fn bento_open_async(
    state: Rc<RefCell<OpState>>,
    #[string] path: String,
    #[scoped] options: Option<OpenOptions>,
) -> Result<ResourceId, FsOpsError> {
    let options = options.unwrap_or_else(OpenOptions::read);
    let (fs, path) = {
        let state = state.borrow();
        let path = checked_open(&state, Cow::Owned(PathBuf::from(path)), options)?;
        (fs(&state).clone(), path.into_owned())
    };
    let file = fs.open_async(path, options).await?;
    Ok(state.borrow_mut().resource_table.add(FileResource::new(file, "fsFile".to_owned())))
}
