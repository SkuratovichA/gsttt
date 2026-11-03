import * as lib from './lib';

export function take() {
  const { a, b: c } = lib;
  return a + c;
}
