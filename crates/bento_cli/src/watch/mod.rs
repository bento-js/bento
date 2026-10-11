//! `bento run --watch`: noticing the source changes that matter to a script, and running it
//! again in a new instance after each burst of them.

mod coalesce;
mod configuration;
mod restarts;
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

pub use self::restarts::run;

/// Watches a project for the changes that should restart its script, and reports each burst
/// of them once. Dropping it stops watching.
pub struct SourceWatcher {
    watcher: RecommendedWatcher,
    scope: Arc<Mutex<WatchScope>>,
    directories_outside_project: BTreeSet<PathBuf>,
    /// Loaded modules being watched.
    modules: BTreeSet<PathBuf>,
    /// Loaded modules that could not be watched and have been reported as such.
    unwatched_modules: BTreeSet<PathBuf>,
}

/// A loaded module that cannot be watched, and why.
pub type UnwatchedModule = (PathBuf, notify::Error);

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
        let scope = Arc::new(Mutex::new(WatchScope::new(project_root)));
        let reported_scope = Arc::clone(&scope);
        thread::spawn(move || coalesce::report_bursts(&notifications, &reported_scope, on_change));
        let mut source_watcher = SourceWatcher {
            watcher,
            scope,
            directories_outside_project: BTreeSet::new(),
            modules: BTreeSet::new(),
            unwatched_modules: BTreeSet::new(),
        };
        source_watcher.watch_configuration()?;
        Ok(source_watcher)
    }

    /// Watches the files `tsconfig.json` is read from as it is now. An edit can make it extend
    /// other files, so this is worth repeating after each change.
    pub fn watch_configuration(&mut self) -> notify::Result<()> {
        let project_root = self.scope.lock().project_root().to_owned();
        configuration::files(&project_root)
            .into_iter()
            // The compiler looks for extended files in directories that need not exist.
            .filter(|file| file.parent().is_some_and(Path::is_dir))
            .try_for_each(|file| self.watch_file(file))
    }

    /// Also watches `modules`, files the script has loaded, except those installed in a
    /// `node_modules` directory. Returns the modules that cannot be watched the first time
    /// each fails; every later call tries them again.
    pub fn watch_modules(
        &mut self,
        modules: impl IntoIterator<Item = PathBuf>,
    ) -> Vec<UnwatchedModule> {
        let new: Vec<PathBuf> = modules
            .into_iter()
            .filter(|module| !scope::is_dependency(module) && !self.modules.contains(module))
            .collect();
        new.into_iter().filter_map(|module| self.watch_module(module)).collect()
    }

    /// Watches `module`, and returns why it cannot be watched unless that was reported before.
    fn watch_module(&mut self, module: PathBuf) -> Option<UnwatchedModule> {
        match self.watch_file(module.clone()) {
            Ok(()) => {
                self.modules.insert(module);
                None
            }
            Err(error) => self.unwatched_modules.insert(module.clone()).then_some((module, error)),
        }
    }

    /// Watches an outside file recursively from its nearest existing ancestor, so creations
    /// of missing parent directories and replacement of the file are noticed.
    fn watch_file(&mut self, file: PathBuf) -> notify::Result<()> {
        let file = file.canonicalize().unwrap_or(file);
        let mut directory = file.parent().expect("a file has a parent directory").to_owned();
        while !directory.is_dir() {
            let Some(parent) = directory.parent() else { break };
            directory = parent.to_owned();
        }
        let mut scope = self.scope.lock();
        scope.add_file(file);
        if directory.starts_with(scope.project_root())
            || self.directories_outside_project.contains(&directory)
        {
            return Ok(());
        }
        self.watcher.watch(&directory, RecursiveMode::Recursive)?;
        self.directories_outside_project.insert(directory);
        Ok(())
    }
}
