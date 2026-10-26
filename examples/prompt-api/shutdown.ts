import assert from "node:assert/strict";

let session = await LanguageModel.create();
let reader = session.promptStreaming("Describe the ocean in five hundred words.").getReader();
assert.equal((await reader.read()).done, false);
console.log("Exiting during native generation; runtime disposal must settle owned callbacks");
process.exit(0);
