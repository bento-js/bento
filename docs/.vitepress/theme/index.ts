import type { Theme } from "vitepress";

import { themeContextKey, VoidZeroTheme } from "@voidzero-dev/vitepress-theme";
import monoIcon from "@voidzero-dev/vitepress-theme/src/assets/icons/vitest-mono.svg";
import footerBg from "@voidzero-dev/vitepress-theme/src/assets/vitest/footer-background.jpg";

import logo from "../../public/bento.png";
import RecordHeader from "./components/RecordHeader.vue";
import RecordLayout from "./layouts/RecordLayout.vue";
import "./custom.css";
import "virtual:group-icons.css";

export default {
    extends: VoidZeroTheme,
    Layout: RecordLayout,
    enhanceApp(ctx) {
        ctx.app.component("RecordHeader", RecordHeader);
        ctx.app.provide(themeContextKey, {
            logoDark: logo,
            logoLight: logo,
            logoAlt: "Bento",
            footerBg,
            monoIcon,
        });
    },
} satisfies Theme;
