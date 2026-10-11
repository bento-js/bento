import { core } from "ext:core/mod.js";

core.ops.op_bento_register_feature_ops(
    core.ops.op_bento_original_worker,
    core.ops.op_bento_original_tcp,
    core.ops.op_bento_original_tls,
);
