use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use deno_core::{ModuleSpecifier, error::CoreError};
use deno_error::JsErrorBox;
use deno_lib::npm::create_npm_process_state_provider;
use deno_lib::worker::{
    LibMainWorker, LibMainWorkerFactory, LibMainWorkerOptions, StorageKeyResolver,
};
use deno_runtime::FeatureChecker;
use deno_runtime::WorkerExecutionMode;
use deno_runtime::WorkerLogLevel;
use deno_runtime::deno_fs::RealFs;
use deno_runtime::deno_os::{ExitCode, WatcherExitHandle, WatcherExited};
use deno_runtime::deno_web::BlobStore;
use deno_tls::{RootCertStoreProvider, rustls::RootCertStore};
use sys_traits::impls::RealSys;

use crate::loader::LoaderFactory;
use crate::node::NodeServices;
use crate::ops::{InstanceEnvironment, bento_instance};
use crate::{RunOutcome, RuntimeConfig, RuntimeError};

pub(crate) async fn run(config: RuntimeConfig, entry: PathBuf) -> RunOutcome {
    let mut worker = match create_worker(config, &entry) {
        Ok(worker) => worker,
        Err(error) => {
            return RunOutcome::Failed(RuntimeError::startup(format!(
                "{}: {error:#}",
                entry.display()
            )));
        }
    };
    let execution = execute(&mut worker).await;
    let state = worker.js_runtime().op_state();
    let state = state.borrow();
    if state.has::<WatcherExited>() {
        return RunOutcome::Exited(state.borrow::<ExitCode>().get());
    }
    match execution {
        Ok(0) => RunOutcome::Completed,
        Ok(code) => RunOutcome::Exited(code),
        Err(error) => RunOutcome::Failed(RuntimeError::guest(error, &entry)),
    }
}

async fn execute(worker: &mut LibMainWorker) -> Result<i32, CoreError> {
    worker.execute_load_phase().await?;
    worker.run_event_loop_to_completion().await
}

fn create_worker(mut config: RuntimeConfig, entry: &Path) -> anyhow::Result<LibMainWorker> {
    let permissions = crate::permissions::container(&config.grants, config.cwd.clone())?;
    let node = NodeServices::new(&config.project_root, permissions.clone());
    let environment = InstanceEnvironment {
        values: std::mem::take(&mut config.env),
        allowed: std::mem::take(&mut config.grants.env),
        cwd: config.cwd.clone(),
    };
    let loader = LoaderFactory { node: node.clone(), cwd: config.cwd.clone() };
    let factory = LibMainWorkerFactory::new(
        Arc::new(BlobStore::default()),
        None,
        None,
        Arc::new(FeatureChecker::default()),
        Arc::new(RealFs),
        None,
        None,
        Box::new(loader),
        node.resolver,
        create_npm_process_state_provider(&node.npm),
        node.packages,
        Arc::new(Certificates(LazyLock::new(deno_tls::create_default_root_cert_store))),
        StorageKeyResolver::empty(),
        RealSys,
        options(config),
        Default::default(),
        None,
    );
    let main = ModuleSpecifier::from_file_path(entry)
        .map_err(|_| anyhow::anyhow!("invalid entry path"))?;
    let mut worker = factory.create_custom_worker(
        WorkerExecutionMode::Run,
        main,
        vec![ModuleSpecifier::parse("bento:compat")?, ModuleSpecifier::parse("bento:subprocess")?],
        Vec::new(),
        permissions,
        vec![bento_instance::init(environment)],
        Default::default(),
        None,
    )?;
    let handle = worker.js_runtime().v8_isolate().thread_safe_handle();
    worker.js_runtime().op_state().borrow_mut().put(WatcherExitHandle(handle));
    Ok(worker)
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
