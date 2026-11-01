export const get = (path: string, base: string) => fetch(new URL(path, base));
