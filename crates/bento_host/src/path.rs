use std::ffi::OsStr;
use std::io;
use std::path::{Component, Path, PathBuf};

/// Links followed in one resolution before giving up, as Linux's `MAXSYMLINKS`. The OS
/// reports real loops itself; this bounds a walk racing concurrent changes to the links.
const MAX_LINKS: u32 = 40;

/// Resolves an absolute path to the form grants are checked against: the location the
/// operating system reaches through it. Existing components resolve as the OS resolves them,
/// so symlinks are followed (dangling ones included) and a `..` after a link leaves the link's
/// target. From the first component that does not exist, the names that follow are kept as
/// written, and a `..` among them removes the name before it. A grant for a file that will
/// be created (a new database, say) therefore names the location its creator will reach.
///
/// # Errors
///
/// [`std::io::Error`] of kind `InvalidInput` when `path` is relative, and whatever the
/// operating system reports for a component it cannot read; a chain of more than 40 links
/// is reported as "too many levels of symbolic links".
pub fn canonical_path(path: &Path) -> io::Result<PathBuf> {
    if !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("grant path `{}` is not absolute", path.display()),
        ));
    }
    match std::fs::canonicalize(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        resolved => return resolved,
    }
    let mut walk = Walk::default();
    walk.follow(path)?;
    Ok(walk.into_path())
}

/// A path walked one component at a time, as the OS walks it.
#[derive(Default)]
struct Walk {
    /// The canonical location reached through components that exist.
    existing: PathBuf,
    /// Names beneath `existing`, the first of which does not exist.
    missing: PathBuf,
    links: u32,
}

impl Walk {
    fn follow(&mut self, path: &Path) -> io::Result<()> {
        for component in path.components() {
            match component {
                Component::Prefix(_) | Component::RootDir => self.existing.push(component),
                Component::CurDir => {}
                Component::ParentDir => self.leave(),
                Component::Normal(name) => self.enter(name)?,
            }
        }
        Ok(())
    }

    fn leave(&mut self) {
        if !self.missing.pop() {
            self.existing.pop();
        }
    }

    fn enter(&mut self, name: &OsStr) -> io::Result<()> {
        if !self.missing.as_os_str().is_empty() {
            self.missing.push(name);
            return Ok(());
        }
        let candidate = self.existing.join(name);
        match std::fs::canonicalize(&candidate) {
            Ok(resolved) => self.existing = resolved,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.enter_absent(&candidate, name)?;
            }
            Err(error) => return Err(error),
        }
        Ok(())
    }

    /// Enters `candidate`, which is either absent or a link to something absent.
    fn enter_absent(&mut self, candidate: &Path, name: &OsStr) -> io::Result<()> {
        match std::fs::read_link(candidate) {
            Ok(target) => self.follow_link(candidate, &target),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.missing.push(name);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn follow_link(&mut self, link: &Path, target: &Path) -> io::Result<()> {
        self.links += 1;
        if self.links > MAX_LINKS {
            return Err(io::Error::other(format!(
                "too many levels of symbolic links at `{}`",
                link.display()
            )));
        }
        self.follow(target)
    }

    fn into_path(self) -> PathBuf {
        if self.missing.as_os_str().is_empty() {
            self.existing
        } else {
            self.existing.join(self.missing)
        }
    }
}
