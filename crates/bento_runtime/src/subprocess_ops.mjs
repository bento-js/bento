import { core } from "ext:core/mod.js";

core.ops.op_bento_register_spawn_ops(
    core.ops.op_bento_original_spawn_child,
    core.ops.op_bento_original_node_spawn_child,
    core.ops.op_bento_original_spawn_sync,
);
