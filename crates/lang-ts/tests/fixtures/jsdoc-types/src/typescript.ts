/**
 * @import { Sel } from "./sel.js";
 * @param {import('./other').Wrapped} a
 */
export function f(a: unknown) {
  return (a as any).compare(1);
}

/** @param {Sel} b */
export function g(b) {
  return b.compare(1);
}
