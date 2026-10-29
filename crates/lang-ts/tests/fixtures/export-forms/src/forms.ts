declare const obj: { b: number; c: number };
const local = 1;
export const a = 1;
export const { b, c: d } = obj;
export declare const g: number;
export function f() {}
export class C {}
export enum E {}
export type T = string;
export interface I {}
export namespace N {
  export const inner = 1;
}
export default 1;
export { local as y };
export { z as w } from './z';
export * as ns from './ns';
export * from './all';
