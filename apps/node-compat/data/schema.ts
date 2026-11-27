import { column as c, table } from "remix/data-table";

export let runs = table({
    name: "compat_runs",
    columns: {
        id: c.text().primaryKey(),
        startedAt: c.text().notNull(),
        summary: c.text().notNull(),
        report: c.text().notNull(),
    },
});
