use std::borrow::Cow;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use deno_core::{OpState, op2};
use deno_runtime::deno_fs::FsOpsError;
use deno_runtime::deno_permissions::OpenAccessKind;

use super::{checked, fs};

#[op2(stack_trace)]
pub(super) fn bento_mkdir_sync(
    state: &mut OpState,
    #[string] path: &str,
    recursive: bool,
    mode: Option<u32>,
) -> Result<(), FsOpsError> {
    let path = checked(state, Cow::Borrowed(Path::new(path)), OpenAccessKind::WriteNoFollow)?;
    fs(state).mkdir_sync(&path, recursive, Some(mode.unwrap_or(0o777) & 0o777))?;
    Ok(())
}

#[op2(stack_trace)]
pub(super) async fn bento_mkdir_async(
    state: Rc<RefCell<OpState>>,
    #[string] path: String,
    recursive: bool,
    mode: Option<u32>,
) -> Result<(), FsOpsError> {
    let (fs, path) = {
        let state = state.borrow();
        let path = checked(&state, Cow::Owned(PathBuf::from(path)), OpenAccessKind::WriteNoFollow)?;
        (fs(&state).clone(), path.into_owned())
    };
    fs.mkdir_async(path, recursive, Some(mode.unwrap_or(0o777) & 0o777)).await?;
    Ok(())
}

#[op2(fast, stack_trace)]
pub(super) fn bento_rmdir_sync(
    state: &mut OpState,
    #[string] path: &str,
) -> Result<(), FsOpsError> {
    let path = checked(state, Cow::Borrowed(Path::new(path)), OpenAccessKind::WriteNoFollow)?;
    fs(state).rmdir_sync(&path)?;
    Ok(())
}

#[op2(stack_trace)]
pub(super) async fn bento_rmdir_async(
    state: Rc<RefCell<OpState>>,
    #[string] path: String,
) -> Result<(), FsOpsError> {
    let (fs, path) = {
        let state = state.borrow();
        let path = checked(&state, Cow::Owned(PathBuf::from(path)), OpenAccessKind::WriteNoFollow)?;
        (fs(&state).clone(), path.into_owned())
    };
    fs.rmdir_async(path).await?;
    Ok(())
}
