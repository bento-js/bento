export type Attributes = Record<string, string>;

export interface Tag {
    attributes: Attributes;
    /** Offset just past the opening tag's closing `>`. */
    end: number;
}

const ATTRIBUTE_PATTERN = /([^\s"'<>/=]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+)))?/g;

const NAMED_ENTITIES: Record<string, string> = {
    amp: "&",
    lt: "<",
    gt: ">",
    quot: '"',
    apos: "'",
};

export function decodeEntities(text: string): string {
    return text.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (entity, body: string) => {
        if (body.startsWith("#x") || body.startsWith("#X")) {
            return String.fromCodePoint(Number.parseInt(body.slice(2), 16));
        }
        if (body.startsWith("#")) return String.fromCodePoint(Number.parseInt(body.slice(1), 10));
        return NAMED_ENTITIES[body.toLowerCase()] ?? entity;
    });
}

/** Opening tags named `name`, in document order, with decoded attribute values. */
export function findTags(html: string, name: string): Tag[] {
    let tagPattern = new RegExp(`<${name}\\b((?:"[^"]*"|'[^']*'|[^'">])*)>`, "gi");
    return [...html.matchAll(tagPattern)].map(match => ({
        attributes: parseAttributes(match[1]!),
        end: match.index + match[0].length,
    }));
}

/** Raw text between the end of `tag` and the next closing `</name>`. */
export function textAfter(html: string, tag: Tag, name: string): string | undefined {
    let closeIndex = html.toLowerCase().indexOf(`</${name}>`, tag.end);
    if (closeIndex === -1) return undefined;
    return html.slice(tag.end, closeIndex);
}

function parseAttributes(source: string): Attributes {
    let attributes: Attributes = {};
    for (let match of source.matchAll(ATTRIBUTE_PATTERN)) {
        let value = match[2] ?? match[3] ?? match[4] ?? "";
        attributes[match[1]!.toLowerCase()] = decodeEntities(value);
    }
    return attributes;
}
