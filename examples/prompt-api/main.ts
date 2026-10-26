import assert from "node:assert/strict";

async function cancelPendingPrompt(session: LanguageModel) {
    let controller = new AbortController();
    let reason = { cancelledBy: "prompt-preview" };
    let response = session.prompt("Describe the ocean in five hundred words.", {
        signal: controller.signal,
    });
    let timeout = setTimeout(() => controller.abort(reason), 100);
    try {
        await assert.rejects(response, error => error === reason);
        console.log("Aborted pending prompt with original reason");
    } finally {
        clearTimeout(timeout);
    }
}

if (process.argv.includes("--denied")) {
    assert.equal(await LanguageModel.availability(), "unavailable");
    await assert.rejects(LanguageModel.create(), { name: "NotAllowedError" });
    console.log("Denied before model access");
} else {
    let availability = await LanguageModel.availability();
    console.log("Availability", availability);
    if (availability !== "available") {
        throw new Error(
            "LanguageModel is unavailable. Check the --allow-language-model grant, that Bento was built with the native backend, macOS 27 or newer, and that Apple Intelligence is enabled and ready.",
        );
    }
    let progress: number[] = [];
    let session = await LanguageModel.create({
        initialPrompts: [
            { role: "system", content: "Answer briefly and plainly." },
            { role: "user", content: "Remember the code word: kelp." },
            { role: "assistant", content: "The code word is kelp." },
        ],
        monitor(monitor) {
            monitor.addEventListener("downloadprogress", event => {
                assert.equal(event.total, 1);
                assert.equal(event.lengthComputable, true);
                progress.push(event.loaded);
            });
        },
    });
    try {
        assert.deepEqual(progress, [0, 1]);
        await session.append([
            { role: "user", content: "The tide is high at noon." },
            { role: "assistant", content: "Noted: high tide at noon." },
        ]);
        let usage = session.contextUsage;
        let measured = await session.measureContextUsage("What is the code word?");
        assert.ok(Number.isFinite(measured) && measured >= 0);
        assert.equal(session.contextUsage, usage);
        console.log("Context", { usage, measured, capacity: session.contextWindow });
        console.log("Recall", await session.prompt("What is the code word?"));
        console.log("Follow-up", await session.prompt("Name a habitat where kelp grows."));

        let branch = await session.clone();
        try {
            assert.equal(branch.contextUsage, session.contextUsage);
            assert.equal(branch.contextWindow, session.contextWindow);
            let chunks = 0;
            for await (let chunk of branch.promptStreaming(
                "Describe a tide pool in four sentences.",
            )) {
                process.stdout.write(chunk);
                chunks++;
            }
            console.log("\nText stream chunks", chunks);
            assert.ok(chunks > 1, "The native text preview must exercise incremental delivery");

            let responseConstraint = {
                type: "object",
                properties: { animal: { type: "string", enum: ["crab", "snail"] } },
                required: ["animal"],
                additionalProperties: false,
            };
            let text = await branch.prompt("Choose a tide-pool animal.", { responseConstraint });
            let answer: { animal: string } = JSON.parse(text);
            assert.ok(["crab", "snail"].includes(answer.animal));
            assert.deepEqual(Object.keys(answer), ["animal"]);
            console.log("Constrained JSON", text);
            let structuredChunks: string[] = [];
            for await (let chunk of branch.promptStreaming("Choose the other animal.", {
                responseConstraint,
                omitResponseConstraintInput: true,
            })) {
                structuredChunks.push(chunk);
            }
            assert.equal(structuredChunks.length, 1);
            assert.ok(["crab", "snail"].includes(JSON.parse(structuredChunks[0]).animal));

            let controller = new AbortController();
            let reason = { cancelledBy: "preview" };
            let reader = branch
                .promptStreaming("Describe the ocean in three hundred words.", {
                    signal: controller.signal,
                })
                .getReader();
            assert.equal((await reader.read()).done, false);
            controller.abort(reason);
            await assert.rejects(reader.read(), error => error === reason);
            reader.releaseLock();
            console.log("Aborted active native stream with original reason");

            let cancelled = branch
                .promptStreaming("Describe the ocean in three hundred words.")
                .getReader();
            assert.equal((await cancelled.read()).done, false);
            await cancelled.cancel();
            console.log("After reader cancellation", await branch.prompt("Say hello briefly."));
            await cancelPendingPrompt(branch);

            session.destroy();
            console.log(
                "Independent clone",
                await branch.prompt("What code word did we remember?"),
            );
        } finally {
            branch.destroy();
        }
    } finally {
        session.destroy();
    }

    let worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
    let { promise, resolve, reject } = Promise.withResolvers<void>();
    worker.onmessage = () => resolve();
    worker.onerror = reject;
    await promise;
    worker.terminate();
    console.log("Terminated worker during native generation");
}
