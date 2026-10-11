export interface GuestBookEntry {
    id: number;
    name: string;
    message: string;
    /**
     * When the entry was saved, as the UTC text SQLite's `CURRENT_TIMESTAMP` produces:
     * `YYYY-MM-DD HH:MM:SS`. Stores convert other timestamp representations to this form.
     */
    createdAt: string;
}

/** The validated fields of a new signature; the store assigns `id` and `createdAt`. */
export type NewGuestBookEntry = Pick<GuestBookEntry, "name" | "message">;

/** Where the guest book keeps its entries. Failures reject; they never read as an empty book. */
export interface GuestBookStore {
    /** Every entry, oldest first. */
    list(): Promise<GuestBookEntry[]>;
    append(entry: NewGuestBookEntry): Promise<void>;
}
