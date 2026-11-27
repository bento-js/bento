export class BoundedLog {
    #head: Buffer[] = [];
    #headLength = 0;
    #tail: Buffer[] = [];
    #tailLength = 0;
    #total = 0;
    readonly #headBytes: number;
    readonly #tailBytes: number;

    constructor(headBytes: number, tailBytes: number) {
        this.#headBytes = headBytes;
        this.#tailBytes = tailBytes;
    }

    get truncated(): boolean {
        return this.#total > this.#headBytes + this.#tailBytes;
    }

    write(chunk: Buffer): void {
        this.#total += chunk.length;
        let headRoom = this.#headBytes - this.#headLength;
        if (headRoom > 0) {
            let head = chunk.subarray(0, headRoom);
            this.#head.push(head);
            this.#headLength += head.length;
            chunk = chunk.subarray(head.length);
        }
        if (chunk.length === 0) return;
        this.#tail.push(chunk);
        this.#tailLength += chunk.length;
        while (
            this.#tail.length > 1 &&
            this.#tailLength - this.#tail[0]!.length >= this.#tailBytes
        ) {
            this.#tailLength -= this.#tail.shift()!.length;
        }
    }

    text(): string {
        let head = Buffer.concat(this.#head).toString("utf8");
        let tail = Buffer.concat(this.#tail);
        if (!this.truncated) return head + tail.toString("utf8");
        let kept = tail.subarray(Math.max(0, tail.length - this.#tailBytes));
        let omitted = this.#total - this.#headLength - kept.length;
        return `${head}\n[… ${omitted} bytes truncated by the collector …]\n${kept.toString("utf8")}`;
    }
}
