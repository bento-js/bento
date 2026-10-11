mod support;

use std::ffi::CStr;

use bento_runtime::{ErrorKind, RunOutcome};

use support::{config, fixture, run};

unsafe extern "C" {
    fn _dyld_image_count() -> u32;
    fn _dyld_get_image_name(index: u32) -> *const std::ffi::c_char;
}

fn loaded_native_addons() -> Vec<String> {
    // dyld owns these immutable, NUL-terminated image names for the process lifetime.
    unsafe {
        (0.._dyld_image_count())
            .filter_map(|index| {
                let name = _dyld_get_image_name(index);
                if name.is_null() {
                    return None;
                }
                let name = CStr::from_ptr(name).to_string_lossy();
                name.ends_with(".node").then(|| name.into_owned())
            })
            .collect()
    }
}

#[test]
fn addon_denial_precedes_dlopen() {
    let root = fixture();
    assert!(loaded_native_addons().is_empty());
    let outcome = run(
        config(root),
        "denied-native.mjs",
        r#"
import { createRequire } from 'node:module';
const require = createRequire(import.meta.resolve('remix/assets'));
await import(require.resolve('oxc-transform'));
"#,
    );
    let RunOutcome::Failed(error) = outcome else {
        panic!("native addon was not denied");
    };
    assert_eq!(error.kind, ErrorKind::Permission, "{error:?}");
    assert!(loaded_native_addons().is_empty(), "addon initialized despite denied authority");
}

#[test]
fn create_require_preserves_denied_addon_error_classification() {
    let outcome = run(
        config(fixture()),
        "denied-require-native.mjs",
        r#"
import { createRequire } from 'node:module';
const require = createRequire(import.meta.resolve('remix/assets'));
require('oxc-transform');
"#,
    );
    assert!(loaded_native_addons().is_empty(), "addon initialized despite denied authority");
    let RunOutcome::Failed(error) = outcome else {
        panic!("native addon was not denied");
    };
    assert_eq!(error.kind, ErrorKind::Permission, "{error:?}");
}
