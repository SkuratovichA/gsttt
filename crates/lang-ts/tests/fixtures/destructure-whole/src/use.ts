import * as lib from './lib';

export function takeRest() {
  const { a, ...rest } = lib;
  return rest;
}

export function takeComputed(key: string) {
  const { [key]: a } = lib;
  return a;
}
