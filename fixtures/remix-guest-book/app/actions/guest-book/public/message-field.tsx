import type { Handle } from "remix/component";

import { clientEntry, css, on } from "remix/component";

import { fieldErrorStyle, fieldHintStyle } from "#app/ui/public/field.ts";
import { inputStyle } from "#app/ui/public/input.ts";
import { theme } from "#app/ui/public/theme.tsx";

const WARNING_THRESHOLD = 20;

export interface MessageFieldProps {
    id: string;
    name: string;
    maxLength: number;
    defaultValue?: string;
    error?: string;
}

export let MessageField = clientEntry(
    import.meta.url,
    function MessageField(handle: Handle<MessageFieldProps>) {
        let length = handle.props.defaultValue?.length ?? 0;

        return () => {
            let { id, name, maxLength, defaultValue, error } = handle.props;
            let remaining = maxLength - length;
            let counterId = `${id}-counter`;
            let errorId = `${id}-error`;

            return (
                <>
                    <textarea
                        aria-describedby={error ? `${errorId} ${counterId}` : counterId}
                        aria-invalid={error ? "true" : undefined}
                        defaultValue={defaultValue}
                        id={id}
                        maxLength={maxLength}
                        mix={[
                            inputStyle,
                            textareaStyle,
                            on("input", event => {
                                length = event.currentTarget.value.length;
                                handle.update();
                            }),
                        ]}
                        name={name}
                        placeholder="Leave a message..."
                        required
                        rows={3}
                    />
                    <div mix={footerStyle}>
                        {error && (
                            <p id={errorId} mix={fieldErrorStyle}>
                                {error}
                            </p>
                        )}
                        <p
                            data-warning={remaining <= WARNING_THRESHOLD ? "" : undefined}
                            id={counterId}
                            mix={[fieldHintStyle, counterStyle]}
                        >
                            {remaining} / {maxLength}
                        </p>
                    </div>
                </>
            );
        };
    },
);

let textareaStyle = css({
    lineHeight: theme.lineHeight.normal,
    resize: "vertical",
});

let footerStyle = css({
    display: "flex",
    alignItems: "baseline",
    gap: theme.space.lg,
});

let counterStyle = css({
    marginInlineStart: "auto",
    "&[data-warning]": {
        color: theme.colors.text.danger,
    },
});
