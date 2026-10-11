const RFC_3339 = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/;

/** Converts an RFC 3339 timestamp to the `GuestBookEntry.createdAt` form, UTC `YYYY-MM-DD HH:MM:SS`. */
export function createdAtFromRfc3339(timestamp: string): string {
    // `Date` reads text without an offset as local time, which would silently shift the entry.
    if (!RFC_3339.test(timestamp)) {
        throw new TypeError(`Expected an RFC 3339 timestamp with an offset, got "${timestamp}"`);
    }
    return new Date(timestamp).toISOString().slice(0, 19).replace("T", " ");
}
