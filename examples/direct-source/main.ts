import { renderToString } from "remix/component/server";

import { type Dish, menu } from "./menu.tsx";

let lunch = [
    { name: "Tamagoyaki", minutes: 10 },
    { name: "Onigiri", minutes: 5 },
] satisfies Dish[];

let html: string = await renderToString(menu(lunch));
console.log(html);
