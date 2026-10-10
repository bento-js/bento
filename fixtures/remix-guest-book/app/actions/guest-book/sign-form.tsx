import type { Handle } from "remix/component";
import type { Issue } from "remix/data-schema";

import { css } from "remix/component";

import type { GuestBookFormValues } from "#app/actions/guest-book/schema.ts";

import { MessageField } from "#app/actions/guest-book/public/message-field.tsx";
import { SignButton } from "#app/actions/guest-book/public/sign-button.tsx";
import { MAX_MESSAGE_LENGTH } from "#app/actions/guest-book/schema.ts";
import { routes } from "#app/routes.ts";
import { fieldErrorStyle, fieldLabelStyle, fieldStyle } from "#app/ui/public/field.ts";
import { inputStyle } from "#app/ui/public/input.ts";
import { theme } from "#app/ui/public/theme.tsx";

export interface SignFormProps {
    /**
     * Frame reloads keep typed values in matching inputs, so a new key swaps in a blank form.
     * Change it after a successful sign; keep it after a failed one so the user's edits survive.
     */
    formKey: string;
    issues?: readonly Issue[];
    values?: GuestBookFormValues;
}

export function SignForm(handle: Handle<SignFormProps>) {
    return () => {
        let { formKey, issues = [], values } = handle.props;
        let nameError = issues.find(issue => issue.path?.[0] === "name")?.message;
        let messageError = issues.find(issue => issue.path?.[0] === "message")?.message;

        return (
            <form
                action={routes.guestBook.action.href()}
                data-rmx-key={formKey}
                method={routes.guestBook.action.method}
                mix={formStyle}
            >
                <div mix={fieldStyle}>
                    <label for="guest-book-name" mix={fieldLabelStyle}>
                        Name
                    </label>
                    <input
                        aria-describedby={nameError ? "guest-book-name-error" : undefined}
                        aria-invalid={nameError ? "true" : undefined}
                        autoComplete="name"
                        defaultValue={values?.name}
                        id="guest-book-name"
                        mix={inputStyle}
                        name="name"
                        placeholder="Your name"
                        required
                    />
                    {nameError && (
                        <p id="guest-book-name-error" mix={fieldErrorStyle}>
                            {nameError}
                        </p>
                    )}
                </div>

                <div mix={fieldStyle}>
                    <label for="guest-book-message" mix={fieldLabelStyle}>
                        Message
                    </label>
                    <MessageField
                        defaultValue={values?.message}
                        error={messageError}
                        id="guest-book-message"
                        maxLength={MAX_MESSAGE_LENGTH}
                        name="message"
                    />
                </div>

                <SignButton />
            </form>
        );
    };
}

let formStyle = css({
    display: "flex",
    flexDirection: "column",
    gap: theme.space.xl,
});
