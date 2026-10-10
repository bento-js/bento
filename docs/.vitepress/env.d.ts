/// <reference types="vite-plus/client" />

declare module "*.vue" {
    import type { DefineComponent } from "vue";
    let component: DefineComponent<{}, {}, unknown>;
    export default component;
}
