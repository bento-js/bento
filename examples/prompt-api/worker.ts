let session = await LanguageModel.create();
try {
    let reader = session.promptStreaming("Describe the ocean in five hundred words.").getReader();
    if ((await reader.read()).done) {
        throw new Error("Native stream ended before worker termination could be exercised");
    }
    postMessage("native-generation-started");
    while (!(await reader.read()).done) {
        // Keep inference active until the parent terminates this isolate.
    }
} finally {
    session.destroy();
}

export {};
