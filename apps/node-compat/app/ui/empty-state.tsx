import type { Handle } from "remix/component";

import { Commands, proseStyle, sectionStyle } from "#app/ui/layout.tsx";

const PREPARE = ["mise run build", "mise run compat:setup", "mise run compat:prepare"];
const BENTO = "--bento ../../target/debug/bento";

const COLLECT = {
    all: `mise run compat:collect -- ${BENTO} --filter buffer`,
    behavior: `mise run compat:collect -- --kind behavior ${BENTO} --filter buffer`,
    inventory: `mise run compat:collect -- --kind inventory ${BENTO}`,
};

export interface EmptyStateProps {
    kind: keyof typeof COLLECT;
    title: string;
}

export function EmptyState(handle: Handle<EmptyStateProps>) {
    return () => {
        let { kind, title } = handle.props;
        return (
            <section aria-labelledby="empty-heading" mix={sectionStyle}>
                <h2 id="empty-heading">{title}</h2>
                <p mix={proseStyle}>
                    Collect evidence from the repository root, then reload this page. The dashboard
                    only shows results the collector recorded; it never starts processes itself.
                </p>
                <Commands label="Collection commands" lines={[...PREPARE, COLLECT[kind]]} />
                <p mix={proseStyle}>
                    <code>--bento</code> is relative to <code>apps/node-compat</code>; point it at
                    the Bento build you want to measure.{" "}
                    {kind === "inventory" ? null : (
                        <>
                            <code>--filter buffer</code> selects test paths containing{" "}
                            <code>buffer</code>; omit it to run the complete corpus, which takes
                            much longer.{" "}
                        </>
                    )}
                    The collector records any explicit <code>--bento-arg</code> permission arguments
                    with the run.
                </p>
            </section>
        );
    };
}
