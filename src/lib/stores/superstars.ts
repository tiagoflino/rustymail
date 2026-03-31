import { writable } from "svelte/store";

export const availableSuperstars = writable<string[]>([]);
