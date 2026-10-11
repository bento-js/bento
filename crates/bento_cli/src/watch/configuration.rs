use std::collections::BTreeSet;
use std::io;
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use oxc_resolver::{
    FileMetadata, FileSystem, FileSystemOs, ResolveError, ResolveOptions, ResolverGeneric,
};
use parking_lot::Mutex;

/// Configuration reads and missing resolution candidates, so repairing either can restart a run.
pub fn files(project_root: &Path) -> BTreeSet<PathBuf> {
    let file_system =
        RecordingFileSystem { project_root: project_root.to_owned(), ..RecordingFileSystem::new() };
    let read = Arc::clone(&file_system.read);
    let resolver = ResolverGeneric::new_with_file_system(file_system, ResolveOptions::default());
    let _ = resolver.resolve_tsconfig(project_root.join("tsconfig.json"));
    mem::take(&mut *read.lock())
}

/// Records reads and failed metadata probes without treating existing directories as inputs.
struct RecordingFileSystem {
    inner: FileSystemOs,
    project_root: PathBuf,
    read: Arc<Mutex<BTreeSet<PathBuf>>>,
}

impl RecordingFileSystem {
    fn record(&self, path: &Path) {
        let mut read = self.read.lock();
        if !read.contains(path) {
            read.insert(path.to_owned());
        }
    }

    fn record_missing(&self, path: &Path) {
        // Failed package lookups above the project would otherwise watch ancestors up to `/`.
        if path.file_name().is_some_and(|name| name == "node_modules")
            && path.parent().is_some_and(|parent| {
                parent != self.project_root && self.project_root.starts_with(parent)
            })
        {
            return;
        }
        self.record(path);
    }
}

impl FileSystem for RecordingFileSystem {
    fn new() -> Self {
        RecordingFileSystem {
            inner: FileSystemOs::new(),
            project_root: PathBuf::new(),
            read: Arc::default(),
        }
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
        let metadata = self.inner.metadata(path);
        if metadata.is_err() {
            self.record_missing(path);
        }
        metadata
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        let metadata = self.inner.symlink_metadata(path);
        if metadata.is_err() {
            self.record_missing(path);
        }
        metadata
    }

    fn read_link(&self, path: &Path) -> Result<PathBuf, ResolveError> {
        self.inner.read_link(path)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.inner.canonicalize(path)
    }
}
