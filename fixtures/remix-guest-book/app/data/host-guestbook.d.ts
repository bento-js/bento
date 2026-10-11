/**
 * Guest-book storage that a Rust embedding host owns and registers for its runtime instance. The
 * stock `bento` CLI does not provide this module. Operations reject when the host refuses them,
 * for example an append to a read-only store.
 */
declare module "host:guestbook" {
    export interface HostGuestBookEntry {
        id: number;
        name: string;
        message: string;
        /** RFC 3339 UTC timestamp, such as `2026-10-10T18:30:05Z`. */
        createdAt: string;
    }

    /** Every entry, oldest first. */
    export function list(): Promise<HostGuestBookEntry[]>;
    export function append(entry: { name: string; message: string }): Promise<void>;
}
