use std::borrow::Cow;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use deno_core::{OpState, op2};
use deno_runtime::deno_fs::{FsOpsError, FsOpsErrorKind};
use deno_runtime::deno_permissions::OpenAccessKind;

use super::stat::Stat;
use super::{checked, fs};

#[op2(fast, stack_trace)]
pub(super) fn bento_lstat_sync(
    state: &mut OpState,
    #[string] path: &str,
    #[buffer] output: &mut [u32],
) -> Result<(), FsOpsError> {
    let path = checked(state, Cow::Borrowed(Path::new(path)), OpenAccessKind::ReadNoFollow)?;
    let stat = Stat::from(fs(state).lstat_sync(&path)?);
    stat.write(output);
    Ok(())
}

#[op2(stack_trace)]
pub(super) async fn bento_lstat_async(
    state: Rc<RefCell<OpState>>,
    #[string] path: String,
) -> Result<Stat, FsOpsError> {
    let (fs, path) = {
        let state = state.borrow();
        let path = checked(&state, Cow::Owned(PathBuf::from(path)), OpenAccessKind::ReadNoFollow)?;
        (fs(&state).clone(), path.into_owned())
    };
    Ok(Stat::from(fs.lstat_async(path).await?))
}

#[op2(stack_trace)]
#[string]
pub(super) fn bento_read_link_sync(
    state: &mut OpState,
    #[string] path: &str,
) -> Result<String, FsOpsError> {
    let path = checked(state, Cow::Borrowed(Path::new(path)), OpenAccessKind::ReadNoFollow)?;
    link_string(fs(state).read_link_sync(&path)?)
}

#[op2(stack_trace)]
#[string]
pub(super) async fn bento_read_link_async(
    state: Rc<RefCell<OpState>>,
    #[string] path: String,
) -> Result<String, FsOpsError> {
    let (fs, path) = {
        let state = state.borrow();
        let path = checked(&state, Cow::Owned(PathBuf::from(path)), OpenAccessKind::ReadNoFollow)?;
        (fs(&state).clone(), path.into_owned())
    };
    link_string(fs.read_link_async(path).await?)
}

fn link_string(path: PathBuf) -> Result<String, FsOpsError> {
    path.into_os_string()
        .into_string()
        .map_err(|path| FsOpsError(Box::new(FsOpsErrorKind::InvalidUtf8(path))))
}
