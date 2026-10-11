import type { Handle, RemixNode } from "remix/component";

export interface Dish {
    name: string;
    minutes: number;
}

function Menu(handle: Handle<{ dishes: Dish[] }>) {
    return () => (
        <ul>
            {handle.props.dishes.map(dish => (
                <li key={dish.name}>
                    {dish.name}: {dish.minutes} minutes
                </li>
            ))}
        </ul>
    );
}

export function menu(dishes: Dish[]): RemixNode {
    return <Menu dishes={dishes} />;
}
