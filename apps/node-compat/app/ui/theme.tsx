import { unsafeHTML } from "remix/component";

const PREFIX = "--rmx";

const TOKENS = {
    fontFamily: {
        sans: 'ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif',
        mono: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace',
    },
    space: {
        xs: "2px",
        sm: "4px",
        md: "8px",
        lg: "12px",
        xl: "16px",
        xxl: "24px",
        xxxl: "40px",
    },
    radius: { sm: "4px", md: "6px", lg: "8px" },
    fontSize: {
        xxs: "11px",
        xs: "12px",
        sm: "13px",
        md: "14px",
        lg: "16px",
        xl: "20px",
    },
    lineHeight: { tight: "1.25", normal: "1.5" },
    letterSpacing: { tight: "-0.015em", meta: "0.025em" },
    fontWeight: { normal: "400", medium: "500", semibold: "600" },
    control: { height: "32px" },
    surface: {
        lvl0: "light-dark(#ffffff, #0a0a0a)",
        lvl1: "light-dark(#f9fafb, #111827)",
        lvl2: "light-dark(#f3f4f6, #1f2937)",
    },
    colors: {
        text: {
            primary: "light-dark(#111827, #f3f4f6)",
            secondary: "light-dark(#374151, #d1d5db)",
            muted: "light-dark(#4b5563, #9ca3af)",
            link: "light-dark(#2563eb, #60a5fa)",
            linkHover: "light-dark(#1e40af, #93c5fd)",
        },
        border: {
            subtle: "light-dark(#e5e7eb, #1f2937)",
            default: "light-dark(#d1d5db, #374151)",
            strong: "light-dark(#9ca3af, #4b5563)",
        },
        focus: { ring: "light-dark(#3b82f6, #60a5fa)" },
        action: {
            background: "#2563eb",
            backgroundHover: "#1d4ed8",
            foreground: "#ffffff",
        },
    },
    status: {
        pass: {
            text: "light-dark(#166534, #86efac)",
            fill: "light-dark(#f0fdf4, #052e16)",
            line: "light-dark(#bbf7d0, #166534)",
        },
        fail: {
            text: "light-dark(#991b1b, #fca5a5)",
            fill: "light-dark(#fef2f2, #450a0a)",
            line: "light-dark(#fecaca, #991b1b)",
        },
        warn: {
            text: "light-dark(#92400e, #fcd34d)",
            fill: "light-dark(#fffbeb, #451a03)",
            line: "light-dark(#fde68a, #92400e)",
        },
        neutral: {
            text: "light-dark(#374151, #d1d5db)",
            fill: "light-dark(#f3f4f6, #1f2937)",
            line: "light-dark(#d1d5db, #4b5563)",
        },
    },
};

type TokenGroup = { [key: string]: string | TokenGroup };

type TokenVars<group extends TokenGroup> = {
    readonly [key in keyof group]: group[key] extends TokenGroup ? TokenVars<group[key]> : string;
};

const UPPER_CASE = /[A-Z]/g;

function toKebabCase(key: string) {
    return key.replace(UPPER_CASE, char => `-${char.toLowerCase()}`);
}

function toVars<group extends TokenGroup>(group: group, prefix: string): TokenVars<group> {
    let vars = Object.fromEntries(
        Object.entries(group).map(([key, value]) => {
            let name = `${prefix}-${toKebabCase(key)}`;
            return [key, typeof value === "string" ? `var(${name})` : toVars(value, name)];
        }),
    );
    return vars as TokenVars<group>;
}

function toDeclarations(group: TokenGroup, prefix: string): string[] {
    return Object.entries(group).flatMap(([key, value]) => {
        let name = `${prefix}-${toKebabCase(key)}`;
        return typeof value === "string" ? `${name}: ${value};` : toDeclarations(value, name);
    });
}

export let theme = toVars(TOKENS, PREFIX);

const CSS_TEXT = [
    // The reset layer must lose to the generated `css()` rules in `rmx`.
    "@layer rmx-reset, rmx;",
    `:root {\n${toDeclarations(TOKENS, PREFIX)
        .map(declaration => `    ${declaration}`)
        .join("\n")}\n}`,
    `@layer rmx-reset {
    body {
        font-family: ${theme.fontFamily.sans};
        font-size: ${theme.fontSize.md};
        line-height: ${theme.lineHeight.normal};
        color: ${theme.colors.text.primary};
        background-color: ${theme.surface.lvl0};
    }
}`,
].join("\n\n");

export function Theme() {
    return () => <style innerHTML={unsafeHTML(CSS_TEXT)} />;
}
