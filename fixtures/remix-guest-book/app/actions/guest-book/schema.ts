import * as s from "remix/data-schema";
import * as f from "remix/data-schema/form-data";

export const MAX_MESSAGE_LENGTH = 280;

// Trim before checking length so whitespace-only values are rejected.
function trimmedText() {
    return s.defaulted(s.string(), "").transform(value => value.trim());
}

export let guestBookForm = f.object({
    name: f.field(trimmedText().refine(name => name.length > 0, "Enter your name.")),
    message: f.field(
        trimmedText()
            .refine(message => message.length > 0, "Write a message to sign the guest book.")
            .refine(
                message => message.length <= MAX_MESSAGE_LENGTH,
                `Keep your message to ${MAX_MESSAGE_LENGTH} characters or fewer.`,
            ),
    ),
});

/** Raw submitted text, echoed back into the form when validation fails. */
export interface GuestBookFormValues {
    name: string;
    message: string;
}
