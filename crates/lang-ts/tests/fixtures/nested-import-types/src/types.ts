export type Wide = typeof import(
  './lib'
);
export type Boxed = import('./lib').Box<typeof import('./other')>;
export const lazy: Promise<typeof import('./lib')> = load();
