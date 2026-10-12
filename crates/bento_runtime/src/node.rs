use std::path::Path;
use std::sync::Arc;

use deno_resolver::cjs::IsCjsResolutionMode;
use deno_resolver::npm::{
    ByonmNpmResolverCreateOptions, CreateInNpmPkgCheckerOptions, DenoInNpmPackageChecker,
    NpmResolver, NpmResolverCreateOptions,
};
use deno_runtime::deno_permissions::PermissionsContainer;
use node_resolver::DenoIsBuiltInNodeModuleChecker;
use node_resolver::analyze::{CjsModuleExportAnalyzer, NodeCodeTranslator, NodeCodeTranslatorMode};
use node_resolver::cache::NodeResolutionSys;
use node_resolver::{NodeResolver, PackageJsonResolver, PackageJsonThreadLocalCache};
use sys_traits::impls::RealSys;

use crate::cjs::Analyzer;
use crate::lifecycle::Control;
use crate::sources::Sources;
use crate::watch_inputs::WatchInputs;

pub(crate) type CjsTracker = deno_resolver::cjs::CjsTracker<DenoInNpmPackageChecker, RealSys>;
pub(crate) type Npm = NpmResolver<RealSys>;
pub(crate) type Resolver =
    NodeResolver<DenoInNpmPackageChecker, DenoIsBuiltInNodeModuleChecker, Npm, RealSys>;
pub(crate) type Translator = NodeCodeTranslator<
    Analyzer,
    DenoInNpmPackageChecker,
    DenoIsBuiltInNodeModuleChecker,
    Npm,
    RealSys,
>;

#[derive(Clone)]
pub(crate) struct NodeServices {
    pub resolver: Arc<Resolver>,
    pub packages: Arc<PackageJsonResolver<RealSys>>,
    pub npm: Npm,
    pub tracker: Arc<CjsTracker>,
    pub translator: Arc<Translator>,
    pub inputs: WatchInputs,
}

impl NodeServices {
    pub fn new(
        project: &Path,
        permissions: PermissionsContainer,
        sources: Arc<Sources>,
        control: &Control,
    ) -> Self {
        let inputs = WatchInputs::new(project.to_owned(), control.watch_inputs());
        let packages = Arc::new(PackageJsonResolver::new(
            RealSys,
            Some(Arc::new(PackageJsonThreadLocalCache)),
        ));
        let checker = DenoInNpmPackageChecker::new(CreateInNpmPkgCheckerOptions::Byonm);
        let resolution_sys = NodeResolutionSys::new(RealSys, None);
        let npm =
            Npm::new::<RealSys>(NpmResolverCreateOptions::Byonm(ByonmNpmResolverCreateOptions {
                sys: resolution_sys.clone(),
                pkg_json_resolver: packages.clone(),
                root_node_modules_dir: Some(project.join("node_modules")),
                search_stop_dir: Some(project.to_owned()),
            }));
        let resolver = Arc::new(NodeResolver::new(
            checker.clone(),
            DenoIsBuiltInNodeModuleChecker,
            npm.clone(),
            packages.clone(),
            resolution_sys,
            Default::default(),
        ));
        let tracker = Arc::new(CjsTracker::new(
            checker.clone(),
            packages.clone(),
            IsCjsResolutionMode::ImplicitTypeCommonJs,
            Vec::new(),
        ));
        let analyzer = Arc::new(CjsModuleExportAnalyzer::new(
            Analyzer { tracker: tracker.clone(), permissions, sources },
            checker,
            resolver.clone(),
            npm.clone(),
            packages.clone(),
            RealSys,
        ));
        let translator =
            Arc::new(NodeCodeTranslator::new(analyzer, NodeCodeTranslatorMode::ModuleLoader));
        Self { resolver, packages, npm, tracker, translator, inputs }
    }
}
