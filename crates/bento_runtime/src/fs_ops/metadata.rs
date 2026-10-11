use std::borrow::Cow;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use deno_core::{OpState, op2};
use deno_runtime::deno_fs::FsOpsError;
use deno_runtime::deno_permissions::OpenAccessKind;

use super::{checked, fs};

#[op2(fast, stack_trace)]
pub(super) fn bento_lutimes_sync(
    state: &mut OpState,
    #[string] path: &str,
    #[number] atime_secs: i64,
    #[smi] atime_nanos: u32,
    #[number] mtime_secs: i64,
    #[smi] mtime_nanos: u32,
) -> Result<(), FsOpsError> {
    let path = checked(state, Cow::Borrowed(Path::new(path)), OpenAccessKind::WriteNoFollow)?;
    fs(state).lutime_sync(&path, atime_secs, atime_nanos, mtime_secs, mtime_nanos)?;
    Ok(())
}

#[op2(stack_trace)]
pub(super) async fn bento_lutimes_async(
    state: Rc<RefCell<OpState>>,
    #[string] path: String,
    #[number] atime_secs: i64,
    #[smi] atime_nanos: u32,
    #[number] mtime_secs: i64,
    #[smi] mtime_nanos: u32,
) -> Result<(), FsOpsError> {
    let (fs, path) = {
        let state = state.borrow();
        let path = checked(&state, Cow::Owned(PathBuf::from(path)), OpenAccessKind::WriteNoFollow)?;
        (fs(&state).clone(), path.into_owned())
    };
    fs.lutime_async(path, atime_secs, atime_nanos, mtime_secs, mtime_nanos).await?;
    Ok(())
}

#[op2(stack_trace)]
pub(super) fn bento_lchown_sync(
    state: &mut OpState,
    #[string] path: &str,
    uid: Option<u32>,
    gid: Option<u32>,
) -> Result<(), FsOpsError> {
    let path = checked(state, Cow::Borrowed(Path::new(path)), OpenAccessKind::WriteNoFollow)?;
    fs(state).lchown_sync(&path, uid, gid)?;
    Ok(())
}

#[op2(stack_trace)]
pub(super) async fn bento_lchown_async(
    state: Rc<RefCell<OpState>>,
    #[string] path: String,
    uid: Option<u32>,
    gid: Option<u32>,
) -> Result<(), FsOpsError> {
    let (fs, path) = {
        let state = state.borrow();
        let path = checked(&state, Cow::Owned(PathBuf::from(path)), OpenAccessKind::WriteNoFollow)?;
        (fs(&state).clone(), path.into_owned())
    };
    fs.lchown_async(path, uid, gid).await?;
    Ok(())
}

#[op2(fast, stack_trace)]
pub(super) fn bento_lchmod_sync(
    state: &mut OpState,
    #[string] path: &str,
    #[smi] mode: u32,
) -> Result<(), FsOpsError> {
    let path = checked(state, Cow::Borrowed(Path::new(path)), OpenAccessKind::WriteNoFollow)?;
    fs(state).lchmod_sync(&path, mode)?;
    Ok(())
}

#[op2(stack_trace)]
pub(super) async fn bento_lchmod_async(
    state: Rc<RefCell<OpState>>,
    #[string] path: String,
    #[smi] mode: u32,
) -> Result<(), FsOpsError> {
    let (fs, path) = {
        let state = state.borrow();
        let path = checked(&state, Cow::Owned(PathBuf::from(path)), OpenAccessKind::WriteNoFollow)?;
        (fs(&state).clone(), path.into_owned())
    };
    fs.lchmod_async(path, mode).await?;
    Ok(())
}
