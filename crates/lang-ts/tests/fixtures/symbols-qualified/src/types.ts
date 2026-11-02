import * as lib from './svc';
import type * as T from './svc';

export function typed(): lib.Foo {}
export function wrapped(): Array<lib.Bar> {}
export function queried(): typeof lib.helper {}
export function onlyType(): T.Foo {}
export function unbound(): Missing.Foo {}

declare function consume(value: unknown): void;
export function escapes(x: lib.Foo) {
  return x;
}
export function passed(x: lib.Foo) {
  consume(x);
}
export function escapesType(x: T.Foo) {
  return x;
}
