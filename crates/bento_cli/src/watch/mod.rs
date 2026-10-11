//! Noticing the source changes that restart a script run with `--watch`.

mod coalesce;
mod configuration;
mod scope;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;

use self::scope::WatchScope;

/// Watches a project for the changes that should restart its script, and reports each burst
/// of them once. Dropping it stops watching.
pub struct SourceWatcher {
    watcher: RecommendedWatcher,
    scope: Arc<Mutex<WatchScope>>,
    directories_outside_project: BTreeSet<PathBuf>,
}

impl SourceWatcher {
    /// Watches `project_root` and the configuration that decides how its modules resolve and
    /// transform, and calls `on_change` with the paths changed in each burst of changes.
    pub fn start(
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
        let scope = Arc::new(Mutex::new(WatchScope::new(project_root.clone())));
        let reported_scope = Arc::clone(&scope);
        thread::spawn(move || coalesce::report_bursts(&notifications, &reported_scope, on_change));
        let mut source_watcher =
            SourceWatcher { watcher, scope, directories_outside_project: BTreeSet::new() };
        source_watcher.watch_files(configuration::files(&project_root))?;
        Ok(source_watcher)
    }

    /// Also watches `modules`, files the script has loaded, except those installed in a
    /// `node_modules` directory.
    pub fn watch_modules(
        &mut self,
        modules: impl IntoIterator<Item = PathBuf>,
    ) -> notify::Result<()> {
        self.watch_files(modules.into_iter().filter(|module| !scope::is_dependency(module)))
    }

    /// Watches each of `files` wherever it is, by watching the directory holding it, so that
    /// replacing or deleting the file is noticed as well as editing it.
    fn watch_files(&mut self, files: impl IntoIterator<Item = PathBuf>) -> notify::Result<()> {
        let mut scope = self.scope.lock();
        for file in files {
            let file = file.canonicalize().unwrap_or(file);
            let directory = file.parent().expect("a file has a parent directory").to_owned();
            scope.add_file(file);
            let watched_with_project = directory.starts_with(scope.project_root());
            if !watched_with_project
                && directory.is_dir()
                && self.directories_outside_project.insert(directory.clone())
            {
                self.watcher.watch(&directory, RecursiveMode::NonRecursive)?;
            }
        }
        Ok(())
    }
}
