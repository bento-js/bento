use std::sync::Arc;

use deno_core::{JsRuntime, RuntimeOptions};
use deno_runtime::deno_fs::{FileSystemRc, RealFs};
use deno_runtime::deno_permissions::{
    Permissions, PermissionsContainer, PermissionsOptions, RuntimePermissionDescriptorParser,
};
use sys_traits::impls::RealSys;

use crate::ops::InstanceEnvironment;

deno_core::extension!(
    removal_test,
    ops = [super::mutations::bento_remove_sync, super::mutations::bento_remove_async],
);

fn runtime(root: &std::path::Path) -> JsRuntime {
    let parser = Arc::new(RuntimePermissionDescriptorParser::new(RealSys));
    let permissions = Permissions::from_options(
        parser.as_ref(),
        &PermissionsOptions {
            allow_write: Some(vec![root.to_string_lossy().into_owned()]),
            deny_write: Some(vec![root.join("tree/protected").to_string_lossy().into_owned()]),
            prompt: false,
            ..Default::default()
        },
    )
    .unwrap();
    let runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![removal_test::init()],
        ..Default::default()
    });
    let state = runtime.op_state();
    let mut state = state.borrow_mut();
    state.put(PermissionsContainer::new(parser, permissions));
    state.put::<FileSystemRc>(Arc::new(RealFs));
    state.put(InstanceEnvironment {
        values: Default::default(),
        allowed: bento_host::Scope::None,
        cwd: root.to_owned(),
    });
    runtime
}

#[tokio::test(flavor = "current_thread")]
async fn recursive_removal_cannot_bypass_a_denied_descendant() {
    for asynchronous in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("tree/protected")).unwrap();
        std::fs::write(root.join("tree/protected/secret"), "keep").unwrap();
        let mut runtime = runtime(&root);
        if asynchronous {
            runtime.execute_script("remove.js", "Deno.core.ops.bento_remove_async('tree', true).then(() => { throw Error('recursive removal bypassed deny'); }, () => {});").unwrap();
            runtime.run_event_loop(Default::default()).await.unwrap();
        } else {
            assert!(
                runtime
                    .execute_script("remove.js", "Deno.core.ops.bento_remove_sync('tree', true)")
                    .is_err(),
                "recursive removal bypassed denied descendant"
            );
        }
        assert_eq!(std::fs::read_to_string(root.join("tree/protected/secret")).unwrap(), "keep");
    }
}
