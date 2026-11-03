import * as lib from './lib';

function helper() {
  return 0;
}

export function take() {
  const { a = helper() } = lib;
  return a;
}
