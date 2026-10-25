use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use deno_core::{Extension, ModuleSpecifier, error::CoreError};
use deno_error::JsErrorBox;
use deno_lib::npm::create_npm_process_state_provider;
use deno_lib::worker::{
    LibMainWorker, LibMainWorkerFactory, LibMainWorkerOptions, StorageKeyResolver,
};
use deno_runtime::WorkerExecutionMode;
use deno_runtime::WorkerLogLevel;
use deno_runtime::deno_fs::RealFs;
use deno_runtime::deno_os::{ExitCode, WatcherExitHandle, WatcherExited};
use deno_runtime::deno_permissions::PermissionsContainer;
use deno_tls::{RootCertStoreProvider, rustls::RootCertStore};
use sys_traits::impls::RealSys;
use tokio::sync::mpsc;

use bento_prompt::deno::{Access, Cleanup};

use crate::host::HostModules;
use crate::lifecycle::{Command, Control, Signal};
use crate::loader::LoaderFactory;
use crate::node::NodeServices;
use crate::ops::{InstanceEnvironment, bento_instance};
use crate::sources::Sources;
use crate::workers::WorkerFactory;
use crate::{RunOutcome, RuntimeConfig, RuntimeError};

pub(crate) async fn run(
    config: RuntimeConfig,
    entry: PathBuf,
    modules: HostModules,
    sources: Arc<Sources>,
    control: Arc<Control>,
    mut commands: mpsc::UnboundedReceiver<Command>,
) -> RunOutcome {
    let disposal_deadline = config.shutdown_deadline;
    let sessions = Cleanup::new();
    let mut worker = match create_worker(
        config,
        &entry,
        modules,
        sources.clone(),
        control.clone(),
        sessions.clone(),
    ) {
        Ok(worker) => worker,
        Err(error) => {
            let outcome = RunOutcome::Failed(RuntimeError::startup(format!(
                "{}: {error:#}",
                entry.display()
            )));
            control.complete(&outcome);
            return outcome;
        }
    };
    let execution = execute(&mut worker, &mut commands, &control).await;
    let outcome = match control.feature_failure() {
        Some(error) => RunOutcome::Failed(error),
        None => execution_outcome(&mut worker, execution, &entry, &sources),
    };
    close_resources(&mut worker);
    drop(worker);
    control.close_workers();
    // Closing a session cancels its model work; the native side still has to finish. Until
    // it has, this instance's disposal is not complete, whatever the isolates look like.
    if tokio::time::timeout(disposal_deadline, sessions.settle()).await.is_err() {
        control.incomplete_disposal();
    }
    match control.complete(&outcome) {
        Some(report) if !matches!(report.outcome, crate::ShutdownOutcome::Failed(_)) => {
            RunOutcome::Shutdown(report)
        }
        _ => outcome,
    }
}

async fn execute(
    worker: &mut LibMainWorker,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    control: &Control,
) -> Result<i32, CoreError> {
    let execution = async {
        worker.execute_load_phase().await?;
        worker.run_event_loop_to_completion().await
    };
    tokio::pin!(execution);
    loop {
        tokio::select! {
            result = &mut execution => return result,
            command = commands.recv() => {
                if !matches!(command, Some(Command::Shutdown)) || !control.deliver(Signal::Terminate.name()) {
                    control.default_shutdown();
                    return Ok(0);
                }
            }
        }
    }
}

fn execution_outcome(
    worker: &mut LibMainWorker,
    execution: Result<i32, CoreError>,
    entry: &Path,
    sources: &Sources,
) -> RunOutcome {
    let state = worker.js_runtime().op_state();
    if state.borrow().has::<WatcherExited>() {
        return RunOutcome::Exited(state.borrow().borrow::<ExitCode>().get());
    }
    match execution {
        Ok(0) => RunOutcome::Completed,
        Ok(code) => RunOutcome::Exited(code),
        Err(error) => RunOutcome::Failed(RuntimeError::guest(error, entry, sources)),
    }
}

fn close_resources(worker: &mut LibMainWorker) {
    let state = worker.js_runtime().op_state();
    let resources = {
        let mut state = state.borrow_mut();
        let ids: Vec<_> = state.resource_table.names().map(|(id, _)| id).collect();
        ids.into_iter()
            .map(|id| {
                state
                    .resource_table
                    .take_any(id)
                    .expect("every id was listed by this table under the same borrow")
            })
            .collect::<Vec<_>>()
    };
    for resource in resources {
        resource.close();
    }
}

fn create_worker(
    mut config: RuntimeConfig,
    entry: &Path,
    modules: HostModules,
    sources: Arc<Sources>,
    control: Arc<Control>,
    sessions: Cleanup,
) -> anyhow::Result<LibMainWorker> {
    let permissions = crate::permissions::container(&config.grants, config.cwd.clone())?;
    let node =
        NodeServices::new(&config.project_root, permissions.clone(), sources.clone(), &control);
    let model = Access::new(config.grants.language_model);
    let environment = instance_environment(&mut config);
    let loader = LoaderFactory {
        node: node.clone(),
        cwd: config.cwd.clone(),
        modules: modules.clone(),
        control: control.clone(),
        sources,
    };
    let workers = Arc::new(WorkerFactory::new(
        loader.clone(),
        environment.clone(),
        config.argv.clone(),
        Arc::new(Certificates(LazyLock::new(deno_tls::create_default_root_cert_store))),
        model,
        sessions.clone(),
    ));
    let factory = main_worker_factory(config, loader, &node, &workers, &control);
    let extensions = vec![
        bento_instance::init(environment, modules, control.clone(), node.inputs),
        crate::feature_ops::extension(workers.callback()),
        bento_prompt::deno::extension(model, sessions),
    ];
    let mut worker = main_worker(&factory, entry, permissions, extensions)?;
    load_require_watch(&mut worker)?;
    install_worker_host(&mut worker, &workers)?;
    install_exit_handle(&mut worker, &control);
    Ok(worker)
}

fn instance_environment(config: &mut RuntimeConfig) -> InstanceEnvironment {
    InstanceEnvironment {
        values: Arc::new(parking_lot::Mutex::new(std::mem::take(&mut config.env))),
        allowed: std::mem::take(&mut config.grants.env),
        cwd: config.cwd.clone(),
    }
}

fn main_worker_factory(
    config: RuntimeConfig,
    loader: LoaderFactory,
    node: &NodeServices,
    workers: &WorkerFactory,
    control: &Arc<Control>,
) -> LibMainWorkerFactory<RealSys> {
    LibMainWorkerFactory::new(
        workers.blobs.clone(),
        None,
        None,
        crate::features::checker(control.clone()),
        Arc::new(RealFs),
        None,
        None,
        Box::new(loader),
        node.resolver.clone(),
        create_npm_process_state_provider(&node.npm),
        node.packages.clone(),
        workers.certificates.clone(),
        StorageKeyResolver::empty(),
        RealSys,
        options(config),
        workers.roots.clone(),
        None,
    )
}

fn main_worker(
    factory: &LibMainWorkerFactory<RealSys>,
    entry: &Path,
    permissions: PermissionsContainer,
    extensions: Vec<Extension>,
) -> anyhow::Result<LibMainWorker> {
    let main = ModuleSpecifier::from_file_path(entry)
        .map_err(|()| anyhow::anyhow!("invalid entry path"))?;
    Ok(factory.create_custom_worker(
        WorkerExecutionMode::Run,
        main,
        vec![ModuleSpecifier::parse("bento:compat")?, ModuleSpecifier::parse("bento:subprocess")?],
        Vec::new(),
        permissions,
        extensions,
        Default::default(),
        None,
    )?)
}

/// `node:module` loads only after bootstrap has initialized `process.platform` and
/// `process.arch`, so the worker must already exist.
fn load_require_watch(worker: &mut LibMainWorker) -> anyhow::Result<()> {
    worker.js_runtime().lazy_load_es_module_with_code(
        "ext:bento_instance/initialize_require_watch.mjs",
        "import { install } from 'ext:bento_instance/require_watch.mjs'; install();",
    )?;
    Ok(())
}

/// The factory's lazy extension initialization resets the worker host's private callback, so
/// ours has to be installed over it afterwards.
fn install_worker_host(
    worker: &mut LibMainWorker,
    workers: &Arc<WorkerFactory>,
) -> anyhow::Result<()> {
    let initialize =
        deno_runtime::ops::worker_host::deno_worker_host::init(workers.callback(), None)
            .op_state_fn
            .ok_or_else(|| anyhow::anyhow!("worker host callback initializer unavailable"))?;
    initialize(&mut worker.js_runtime().op_state().borrow_mut());
    Ok(())
}

fn install_exit_handle(worker: &mut LibMainWorker, control: &Control) {
    let handle = worker.js_runtime().v8_isolate().thread_safe_handle();
    control.install_isolate(handle.clone());
    worker.js_runtime().op_state().borrow_mut().put(WatcherExitHandle(handle));
}

fn options(config: RuntimeConfig) -> LibMainWorkerOptions {
    LibMainWorkerOptions {
        argv: config.argv,
        argv0: Some("bento".into()),
        log_level: WorkerLogLevel::Info,
        enable_raw_imports: false,
        enable_testing_features: false,
        has_node_modules_dir: true,
        inspect_brk: false,
        inspect_wait: false,
        trace_ops: None,
        is_inspecting: false,
        is_standalone: true,
        auto_serve: false,
        location: None,
        node_debug: None,
        node_cluster_unique_id: None,
        node_cluster_sched_policy: None,
        otel_config: Default::default(),
        origin_data_folder_path: None,
        seed: None,
        unsafely_ignore_certificate_errors: None,
        skip_op_registration: false,
        node_ipc_init: None,
        no_legacy_abort: false,
        startup_snapshot: deno_snapshots::CLI_SNAPSHOT,
        residual_lazy_js_sources: deno_snapshots::RESIDUAL_LAZY_JS,
        residual_lazy_esm_sources: deno_snapshots::RESIDUAL_LAZY_ESM,
        serve_port: None,
        serve_host: None,
        close_on_idle: true,
        maybe_initial_cwd: ModuleSpecifier::from_directory_path(config.cwd).ok(),
        disable_offscreen_canvas: false,
    }
}

struct Certificates(LazyLock<RootCertStore>);
impl RootCertStoreProvider for Certificates {
    fn get_or_try_init(&self) -> Result<&RootCertStore, JsErrorBox> {
        Ok(&self.0)
    }
}
