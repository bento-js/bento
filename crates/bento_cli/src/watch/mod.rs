//! `bento run --watch`: noticing the source changes that matter to a script, and running it
//! again in a new instance after each burst of them.

mod coalesce;
mod configuration;
mod restarts;
mod scope;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;

use self::scope::WatchScope;

pub(crate) use self::restarts::run;

/// Watches a project for the changes that should restart its script, and reports each burst
/// of them once. Dropping it stops watching.
pub(crate) struct SourceWatcher {
    watcher: RecommendedWatcher,
    scope: Arc<Mutex<WatchScope>>,
    directories_outside_project: BTreeMap<PathBuf, RecursiveMode>,
    watched_files: BTreeSet<PathBuf>,
    /// Failed watch registrations are retried without repeating their warnings.
    unwatched_files: BTreeSet<PathBuf>,
}

/// A source or configuration file that cannot be watched, and why.
pub(crate) type UnwatchedFile = (PathBuf, notify::Error);

impl SourceWatcher {
    /// Watches `project_root` and calls `on_change` once for each burst of source changes.
    pub(crate) fn start(
        project_root: &Path,
        on_change: impl FnMut(Vec<PathBuf>) + Send + 'static,
    ) -> notify::Result<SourceWatcher> {
        // Notifications name real paths, so the root they are matched against must be one too.
        let project_root = project_root.canonicalize()?;
        let (sender, notifications) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |notification| {
            // Sending fails only once watching has stopped and nobody is listening.
            let _ = sender.send(notification);
        })?;
        watcher.watch(&project_root, RecursiveMode::Recursive)?;
        let scope = Arc::new(Mutex::new(WatchScope::new(project_root)));
        let reported_scope = Arc::clone(&scope);
        thread::spawn(move || coalesce::report_bursts(&notifications, &reported_scope, on_change));
        Ok(SourceWatcher {
            watcher,
            scope,
            directories_outside_project: BTreeMap::new(),
            watched_files: BTreeSet::new(),
            unwatched_files: BTreeSet::new(),
        })
    }

    /// Watches the files `tsconfig.json` is read from as it is now. An edit can make it extend
    /// other files, so this is worth repeating after each change.
    pub(crate) fn watch_configuration(&mut self) -> Vec<UnwatchedFile> {
        let project_root = self.scope.lock().project_root().to_owned();
        configuration::files(&project_root)
            .into_iter()
            .filter_map(|file| self.watch_input(file))
            .collect()
    }

    /// Returns each registration failure once and retries it on later calls.
    /// Dependency checks ignore ancestors of the project, which may itself be in `node_modules`.
    pub(crate) fn watch_inputs(
        &mut self,
        inputs: impl IntoIterator<Item = PathBuf>,
    ) -> Vec<UnwatchedFile> {
        let mut failures = Vec::new();
        for input in inputs {
            if self.watched_files.contains(&input)
                || scope::is_dependency(self.scope.lock().project_root(), &input)
            {
                continue;
            }
            if let Some(failure) = self.watch_input(input) {
                failures.push(failure);
            }
        }
        failures
    }

    /// Retries registration but returns each failure only once.
    fn watch_input(&mut self, file: PathBuf) -> Option<UnwatchedFile> {
        match self.watch_file(&file) {
            Ok(()) => {
                self.watched_files.insert(file);
                None
            }
            Err(error) => self.unwatched_files.insert(file.clone()).then_some((file, error)),
        }
    }

    /// Missing parent directories need recursive coverage; existing package-scope parents do not.
    fn watch_file(&mut self, file: &Path) -> notify::Result<()> {
        let mut ancestor = file;
        let file = loop {
            match ancestor.canonicalize() {
                Ok(canonical) => {
                    break canonical.join(file.strip_prefix(ancestor).expect("a path's ancestor"));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    ancestor = ancestor.parent().ok_or_else(|| notify::Error::io(error))?;
                }
                Err(error) => return Err(notify::Error::io(error)),
            }
        };
        let mut directory = file.parent().expect("a file has a parent directory").to_owned();
        while !directory.is_dir() {
            let Some(parent) = directory.parent() else { break };
            directory = parent.to_owned();
        }
        let mode = if file.file_name().is_some_and(|name| name == "package.json")
            && file.parent() == Some(directory.as_path())
        {
            RecursiveMode::NonRecursive
        } else {
            RecursiveMode::Recursive
        };
        let mut scope = self.scope.lock();
        scope.add_file(file);
        if directory.starts_with(scope.project_root())
            || self.directories_outside_project.get(&directory).is_some_and(|watched_mode| {
                *watched_mode == RecursiveMode::Recursive || *watched_mode == mode
            })
        {
            return Ok(());
        }
        self.watcher.watch(&directory, mode)?;
        self.directories_outside_project.insert(directory, mode);
        Ok(())
    }
}
