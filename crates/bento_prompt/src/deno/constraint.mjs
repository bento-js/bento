// The documented JSON Schema subset `responseConstraint` accepts, settled before a session is
// touched. What the subset admits, and which failure an unacceptable schema is, is decided by
// the same Rust translation a generation uses, over the same snapshot.

import { webError } from "ext:bento_prompt_deno/input.mjs";
import { core } from "ext:core/mod.js";

let { op_bento_prompt_constraint } = core.ops;

function acceptConstraint(constraint) {
    if (constraint === undefined) {
        return null;
    }
    // JSON.stringify is the gate for values that cannot travel as JSON, such as cycles and
    // BigInts. Parsing its text back is also what every later step reads: a guest's getters
    // and proxies run here, once, and never inside a native call, which would re-enter the
    // runtime from an operation that is not expecting it. A JSON round trip inside
    // JavaScript keeps every number exactly as the guest wrote it.
    let text = JSON.stringify(constraint);
    if (text === undefined) {
        throw new TypeError("responseConstraint must be a JSON Schema object");
    }
    let snapshot = JSON.parse(text);
    let refused = op_bento_prompt_constraint(snapshot);
    if (refused !== null) {
        throw webError(refused);
    }
    return snapshot;
}

/** What one generation or measurement asks of the model, beyond its messages. */
export function promptRequest(options) {
    return {
        responseConstraint: acceptConstraint(options.responseConstraint),
        omitResponseConstraintInput: options.omitResponseConstraintInput,
    };
}
