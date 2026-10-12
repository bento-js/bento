use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use deno_core::{error::CoreError, v8};
use deno_lib::npm::create_npm_process_state_provider;
use deno_lib::worker::{LibWorkerFactoryRoots, ModuleLoaderFactory};
use deno_node::{NodeExtInitServices, ops::worker_threads::ResolvedResourceLimits};
use deno_resolver::npm::DenoInNpmPackageChecker;
use deno_runtime::deno_fs::RealFs;
use deno_runtime::deno_os::WatcherExitHandle;
use deno_runtime::deno_permissions::PermissionsContainer;
use deno_runtime::deno_web::BlobStore;
use deno_runtime::ops::worker_host::{CreateWebWorkerArgs, CreateWebWorkerCb, ResourceLimits};
use deno_runtime::web_worker::{
    SendableWebWorkerHandle, WebWorker, WebWorkerOptions, WebWorkerServiceOptions,
};
use deno_runtime::{BootstrapOptions, WorkerExecutionMode};
use deno_tls::RootCertStoreProvider;
use sys_traits::impls::RealSys;

use crate::lifecycle::Control;
use crate::loader::LoaderFactory;
use crate::ops::{InstanceEnvironment, bento_instance};

pub(crate) struct WorkerFactory {
    loader: LoaderFactory,
    environment: InstanceEnvironment,
    argv: Vec<String>,
    pub roots: LibWorkerFactoryRoots,
    pub blobs: Arc<BlobStore>,
    pub certificates: Arc<dyn RootCertStoreProvider>,
}

impl WorkerFactory {
    pub fn new(
        loader: LoaderFactory,
        environment: InstanceEnvironment,
        argv: Vec<String>,
        certificates: Arc<dyn RootCertStoreProvider>,
    ) -> Self {
        Self {
            loader,
            environment,
            argv,
            roots: Default::default(),
            blobs: Default::default(),
            certificates,
        }
    }

    pub fn callback(self: &Arc<Self>) -> Arc<CreateWebWorkerCb> {
        let factory = self.clone();
        Arc::new(move |args| factory.create(args))
    }

    fn create(self: &Arc<Self>, args: CreateWebWorkerArgs) -> (WebWorker, SendableWebWorkerHandle) {
        let loaders = self.loader.create_for_worker(
            args.parent_permissions,
            args.permissions.clone(),
            args.maybe_main_module_blob.clone().map(|blob| (args.main_module.clone(), blob)),
        );
        let services = self.services(loaders, args.permissions);
        let (create_params, resolved_limits) = resource_limits(args.resource_limits.as_ref());
        let options = WebWorkerOptions {
            name: args.name,
            main_module: args.main_module.clone(),
            worker_id: args.worker_id,
            bootstrap: BootstrapOptions {
                args: self.argv.clone(),
                location: Some(args.main_module),
                is_standalone: true,
                has_node_modules_dir: true,
                argv0: Some("bento".into()),
                mode: WorkerExecutionMode::Worker,
                close_on_idle: args.close_on_idle,
                ..Default::default()
            },
            extensions: vec![
                bento_instance::init(
                    self.environment.clone(),
                    self.loader.modules.clone(),
                    self.loader.control.clone(),
                    self.loader.node.inputs.clone(),
                ),
                crate::feature_ops::extension(self.callback()),
            ],
            startup_snapshot: deno_snapshots::CLI_SNAPSHOT,
            residual_lazy_js_sources: deno_snapshots::RESIDUAL_LAZY_JS,
            residual_lazy_esm_sources: deno_snapshots::RESIDUAL_LAZY_ESM,
            unsafely_ignore_certificate_errors: None,
            create_params,
            seed: None,
            create_web_worker_cb: self.callback(),
            format_js_error_fn: None,
            worker_type: args.worker_type,
            cache_storage_dir: None,
            stdio: Default::default(),
            trace_ops: None,
            close_on_idle: args.close_on_idle,
            maybe_worker_metadata: args.maybe_worker_metadata,
            maybe_main_module_blob: args.maybe_main_module_blob,
            maybe_coverage_dir: None,
            maybe_cpu_prof_config: None,
            enable_raw_imports: false,
            enable_stack_trace_arg_in_ops: false,
            wait_for_debugger_on_start: args.wait_for_debugger_on_start,
            wait_for_page_wait_for_debugger: args.wait_for_page_wait_for_debugger,
        };
        let (mut worker, handle, bootstrap) = WebWorker::from_options(services, options);
        if let Err(error) = self.bootstrap(&mut worker, &bootstrap, resolved_limits) {
            self.loader.control.fail_feature(crate::RuntimeError::startup(format!(
                "worker require-watch initialization: {error}"
            )));
        }
        (worker, handle)
    }

    fn services(
        &self,
        loaders: deno_lib::worker::CreateModuleLoaderResult,
        permissions: PermissionsContainer,
    ) -> WebWorkerServiceOptions<DenoInNpmPackageChecker, crate::node::Npm, RealSys> {
        let node = &self.loader.node;
        WebWorkerServiceOptions {
            blob_store: self.blobs.clone(),
            broadcast_channel: Default::default(),
            deno_rt_native_addon_loader: None,
            compiled_wasm_module_store: Some(self.roots.compiled_wasm_module_store.clone()),
            feature_checker: crate::features::checker(self.loader.control.clone()),
            fs: Arc::new(RealFs),
            main_inspector_session_tx: Default::default(),
            module_loader: loaders.module_loader,
            node_services: Some(NodeExtInitServices {
                node_require_loader: loaders.node_require_loader,
                node_resolver: node.resolver.clone(),
                pkg_json_resolver: node.packages.clone(),
                sys: RealSys,
            }),
            npm_process_state_provider: Some(create_npm_process_state_provider(&node.npm)),
            permissions,
            root_cert_store_provider: Some(self.certificates.clone()),
            shared_array_buffer_store: Some(self.roots.shared_array_buffer_store.clone()),
            bundle_provider: None,
        }
    }

    fn bootstrap(
        &self,
        worker: &mut WebWorker,
        bootstrap: &BootstrapOptions,
        resolved_limits: Option<ResolvedResourceLimits>,
    ) -> Result<(), CoreError> {
        let isolate = worker.js_runtime.v8_isolate().thread_safe_handle();
        let control = self.loader.control.clone();
        let lifetime = WorkerLifetime { id: control.register_worker(isolate.clone()), control };
        WORKER_LIFETIME.with(|slot| *slot.borrow_mut() = Some(lifetime));
        let limited = resolved_limits.is_some();
        {
            let state = worker.js_runtime.op_state();
            let mut state = state.borrow_mut();
            state.put(WorkerIsolate);
            state.put(WatcherExitHandle(isolate.clone()));
            if let Some(limits) = resolved_limits {
                state.put(limits);
            }
        }
        if limited {
            let oom = worker.oom_triggered.clone();
            worker.js_runtime.add_near_heap_limit_callback(move |limit, _| {
                oom.store(true, Ordering::SeqCst);
                isolate.terminate_execution();
                limit * 2
            });
        }
        worker.bootstrap(bootstrap);
        worker.js_runtime.lazy_load_es_module_with_code(
            "ext:bento_instance/initialize_require_watch.mjs",
            "import { install } from 'ext:bento_instance/require_watch.mjs'; install();",
        )?;
        Ok(())
    }
}

pub(crate) struct WorkerIsolate;

// OpState clears extension state before resources. Thread-local destruction happens after
// the worker's isolate, pending native operations, and Tokio runtime have been disposed.
thread_local! {
    static WORKER_LIFETIME: RefCell<Option<WorkerLifetime>> = const { RefCell::new(None) };
}

struct WorkerLifetime {
    id: u64,
    control: Arc<Control>,
}

impl Drop for WorkerLifetime {
    fn drop(&mut self) {
        self.control.finish_worker(self.id);
    }
}

fn resource_limits(
    limits: Option<&ResourceLimits>,
) -> (Option<v8::CreateParams>, Option<ResolvedResourceLimits>) {
    let Some(limits) = limits else {
        return (None, None);
    };
    let mut params = v8::CreateParams::default();
    if let Some(size) = limits.max_old_generation_size_mb.filter(|size| *size > 0) {
        params = params.set_max_old_generation_size_in_bytes(size * 1024 * 1024);
    }
    if let Some(size) = limits.max_young_generation_size_mb.filter(|size| *size > 0) {
        params = params.set_max_young_generation_size_in_bytes(size * 1024 * 1024);
    }
    if let Some(size) = limits.code_range_size_mb.filter(|size| *size > 0) {
        params = params.set_code_range_size_in_bytes(size * 1024 * 1024);
    }
    let resolved = ResolvedResourceLimits {
        max_young_generation_size_mb: params.max_young_generation_size_in_bytes() / (1024 * 1024),
        max_old_generation_size_mb: params.max_old_generation_size_in_bytes() / (1024 * 1024),
        code_range_size_mb: params.code_range_size_in_bytes() / (1024 * 1024),
        stack_size_mb: limits
            .stack_size_mb
            .unwrap_or(deno_node::ops::worker_threads::DEFAULT_STACK_SIZE_MB),
    };
    (Some(params), Some(resolved))
}
