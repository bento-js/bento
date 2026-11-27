import type { Handle } from "remix/component";

import { routes } from "#app/routes.ts";
import { Document } from "#app/ui/document.tsx";
import { PageHeader, proseStyle } from "#app/ui/layout.tsx";

export interface NotFoundPageProps {
    title: string;
    message: string;
}

export function NotFoundPage(handle: Handle<NotFoundPageProps>) {
    return () => {
        let { title, message } = handle.props;
        return (
            <Document section="history" title={title}>
                <PageHeader title={title}>
                    <p mix={proseStyle}>
                        {message} Runs listed in <a href={routes.runs.index.href()}>History</a> are
                        the ones saved in this dashboard's database.
                    </p>
                </PageHeader>
            </Document>
        );
    };
}
