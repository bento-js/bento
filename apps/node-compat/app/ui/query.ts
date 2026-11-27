export const PAGE_SIZE = 250;

export interface ResultQuery<status extends string> {
    module: string;
    status: status | "";
    q: string;
    page: number;
}

export function readQuery<status extends string>(
    url: URL,
    statuses: readonly status[],
): ResultQuery<status> {
    let status = url.searchParams.get("status") ?? "";
    let page = Number(url.searchParams.get("page") ?? "1");
    return {
        module: url.searchParams.get("module") ?? "",
        status: (statuses as readonly string[]).includes(status) ? (status as status) : "",
        q: (url.searchParams.get("q") ?? "").trim(),
        page: Number.isSafeInteger(page) && page > 0 ? page : 1,
    };
}

export function queryParams(query: ResultQuery<string>, page?: number) {
    return {
        module: query.module,
        status: query.status,
        q: query.q,
        page: page === undefined || page === 1 ? undefined : page,
    };
}

export function containsText(value: string, needle: string): boolean {
    return needle === "" || value.toLowerCase().includes(needle.toLowerCase());
}

export interface Page<item> {
    items: item[];
    page: number;
    pages: number;
    total: number;
    start: number;
}

export function paginate<item>(items: item[], page: number, size = PAGE_SIZE): Page<item> {
    let pages = Math.max(1, Math.ceil(items.length / size));
    let current = Math.min(page, pages);
    let start = (current - 1) * size;
    return {
        items: items.slice(start, start + size),
        page: current,
        pages,
        total: items.length,
        start,
    };
}
