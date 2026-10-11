use std::sync::Arc;

use deno_runtime::FeatureChecker;

use crate::RuntimeError;
use crate::lifecycle::Control;

pub(crate) fn checker(control: Arc<Control>) -> Arc<FeatureChecker> {
    let mut checker = FeatureChecker::default();
    checker.set_exit_cb(Box::new(move |feature, api| {
        control.fail_feature(RuntimeError::unsupported(format!(
            "NotSupported: {api} requires unsupported feature '{feature}'"
        )));
    }));
    Arc::new(checker)
}
