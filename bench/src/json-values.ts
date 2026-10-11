/** Reads a nested property from parsed JSON, or undefined when any step is not an object. */
export function lookup(value: unknown, ...path: string[]): unknown {
    let current = value;
    for (let key of path) {
        if (typeof current !== "object" || current === null || !(key in current)) return undefined;
        current = Reflect.get(current, key);
    }
    return current;
}

export function readNumber(value: unknown, ...path: string[]): number {
    let found = lookup(value, ...path);
    if (typeof found !== "number") throw new Error(`Expected a number at ${path.join(".")}`);
    return found;
}

export function readString(value: unknown, ...path: string[]): string {
    let found = lookup(value, ...path);
    if (typeof found !== "string") throw new Error(`Expected a string at ${path.join(".")}`);
    return found;
}

export function readArray(value: unknown, ...path: string[]): unknown[] {
    let found = lookup(value, ...path);
    if (!Array.isArray(found)) throw new Error(`Expected an array at ${path.join(".")}`);
    return found;
}
