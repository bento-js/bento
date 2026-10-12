import * as s from "remix/data-schema";
import { redirect } from "remix/response/redirect";
import { createController } from "remix/router";

import { GuestBookPage } from "#app/actions/guest-book/page.tsx";
import { guestBookForm } from "#app/actions/guest-book/schema.ts";
import { routes } from "#app/routes.ts";

export default createController(routes.guestBook, {
    actions: {
        async index({ guestBook, render }) {
            let entries = await guestBook.list();
            return render(<GuestBookPage entries={entries} />);
        },
        async action({ formData, guestBook, render }) {
            let submission = s.parseSafe(guestBookForm, formData);

            if (!submission.success) {
                let entries = await guestBook.list();
                let values = {
                    name: readText(formData, "name"),
                    message: readText(formData, "message"),
                };

                return render(
                    <GuestBookPage entries={entries} issues={submission.issues} values={values} />,
                    { status: 400 },
                );
            }

            await guestBook.append(submission.value);
            return redirect(routes.guestBook.index.href(), 303);
        },
    },
});

function readText(formData: FormData, name: string): string {
    let value = formData.get(name);
    return typeof value === "string" ? value : "";
}
