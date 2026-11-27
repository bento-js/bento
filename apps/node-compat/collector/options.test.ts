import * as assert from "remix/assert";
import { it } from "remix/test";

import { SetupError } from "./errors.ts";
import { DEFAULT_TIMEOUT_MS, parseOptions } from "./options.ts";

it("collects inventory and behavior with the default Bento command", () => {
    assert.deepEqual(parseOptions([]), {
        kinds: ["inventory", "behavior"],
        filter: null,
        bento: "bento",
        bentoArguments: [],
        timeoutMs: DEFAULT_TIMEOUT_MS,
    });
});

it("keeps repeated Bento arguments in order", () => {
    let options = parseOptions([
        "--kind",
        "behavior",
        "--filter",
        "buffer",
        "--bento",
        "../../target/debug/bento",
        "--bento-arg=--permission",
        "--bento-arg=--allow-net",
        "--timeout-ms",
        "5000",
    ]);
    assert.deepEqual(options, {
        kinds: ["behavior"],
        filter: "buffer",
        bento: "../../target/debug/bento",
        bentoArguments: ["--permission", "--allow-net"],
        timeoutMs: 5000,
    });
});

it("rejects invalid or ambiguous selections before collecting", () => {
    let invalid = [
        ["--kind", "everything"],
        ["--filter="],
        ["--kind", "inventory", "--filter", "buffer"],
        ["--timeout-ms", "0"],
        ["--timeout-ms", "1.5"],
        ["--timeout-ms", "soon"],
        ["--bento-arg", "--permission"],
        ["--unknown"],
        ["stray.js"],
    ];
    for (let argv of invalid) assert.throws(() => parseOptions(argv), SetupError, argv.join(" "));
});

it("returns null when usage was requested", () => {
    assert.equal(parseOptions(["--help"]), null);
});
