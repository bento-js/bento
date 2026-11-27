import { CORPUS_DIRECTORIES, listCorpus } from "./corpus.ts";
import { SetupError } from "./errors.ts";
import { CACHE_DIRECTORY, prepareUpstream } from "./upstream.ts";

try {
    await prepareUpstream(line => console.log(line));
    let corpus = await listCorpus(CACHE_DIRECTORY);
    let directories = CORPUS_DIRECTORIES.map(directory => `test/${directory}`).join(" and ");
    console.log(`Behavioral corpus: ${corpus.length} test-* JavaScript files in ${directories}.`);
} catch (error) {
    if (!(error instanceof SetupError)) throw error;
    console.error(`error: ${error.message}`);
    process.exitCode = 1;
}
