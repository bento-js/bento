use std::collections::BTreeSet;
use std::io;
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use oxc_resolver::{
    FileMetadata, FileSystem, FileSystemOs, ResolveError, ResolveOptions, ResolverGeneric,
};
use parking_lot::Mutex;

/// The files `tsconfig.json` is read from: itself and every file it extends, directly or
/// through other extended files, with the `package.json` files that locate extended packages.
///
/// They are found by loading `tsconfig.json` the way the compiler loads it and recording
/// what that reads. A configuration that fails to load still yields the files read before
/// the failure, so that fixing one of them counts as a change.
pub fn files(project_root: &Path) -> BTreeSet<PathBuf> {
    let file_system = RecordingFileSystem::new();
    let read = Arc::clone(&file_system.read);
    let resolver = ResolverGeneric::new_with_file_system(file_system, ResolveOptions::default());
    let _ = resolver.resolve_tsconfig(project_root.join("tsconfig.json"));
    mem::take(&mut *read.lock())
}

/// The operating system's file system, remembering every file read through it.
struct RecordingFileSystem {
    inner: FileSystemOs,
    read: Arc<Mutex<BTreeSet<PathBuf>>>,
}

impl RecordingFileSystem {
    fn record(&self, path: &Path) {
        self.read.lock().insert(path.to_owned());
    }
}

impl FileSystem for RecordingFileSystem {
    fn new() -> Self {
        RecordingFileSystem { inner: FileSystemOs::new(), read: Arc::default() }
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.record(path);
        self.inner.read(path)
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        self.record(path);
        self.inner.read_to_string(path)
    }

    fn metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        self.inner.metadata(path)
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        self.inner.symlink_metadata(path)
    }

    fn read_link(&self, path: &Path) -> Result<PathBuf, ResolveError> {
        self.inner.read_link(path)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.inner.canonicalize(path)
    }
}
