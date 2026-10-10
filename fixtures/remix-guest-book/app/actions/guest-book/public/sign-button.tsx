import type { Handle } from "remix/component";

import { clientEntry, css } from "remix/component";

import { button } from "#app/ui/public/button.ts";

export let SignButton = clientEntry(import.meta.url, function SignButton(handle: Handle) {
    let pending = false;

    handle.frame.addEventListener(
        "reloadStart",
        () => {
            pending = true;
            handle.update();
        },
        { signal: handle.signal },
    );

    handle.frame.addEventListener(
        "reloadComplete",
        () => {
            pending = false;
            handle.update();
        },
        { signal: handle.signal },
    );

    return () => (
        <button
            disabled={pending}
            mix={[button({ tone: "primary" }), placementStyle]}
            type="submit"
        >
            {pending ? "Signing…" : "Sign"}
        </button>
    );
});

// Fits "Signing…" so the label swap doesn't shift the button.
let placementStyle = css({
    alignSelf: "flex-end",
    minWidth: "6rem",
});
