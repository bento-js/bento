// The documented JSON Schema subset `responseConstraint` accepts, settled before a session is
// touched. What the subset admits, and which failure an unacceptable schema is, is decided by
// the same Rust translation a generation uses, over the same converted value.

import { webError } from "ext:bento_prompt_deno/input.mjs";
import { core } from "ext:core/mod.js";

let { op_bento_prompt_constraint } = core.ops;

function acceptConstraint(constraint) {
    if (constraint === undefined) {
        return null;
    }
    // JSON.stringify is only the gate for values that cannot travel as JSON, such as cycles
    // and BigInts; its text is not what the model receives.
    if (JSON.stringify(constraint) === undefined) {
        throw new TypeError("responseConstraint must be a JSON Schema object");
    }
    let refused = op_bento_prompt_constraint(constraint);
    if (refused !== null) {
        throw webError(refused);
    }
    return constraint;
}

/** What one generation or measurement asks of the model, beyond its messages. */
export function promptRequest(options) {
    return {
        responseConstraint: acceptConstraint(options.responseConstraint),
        omitResponseConstraintInput: options.omitResponseConstraintInput,
    };
}
