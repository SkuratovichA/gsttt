import { decorate, track } from './decorators';

const {
  first,
  second: renamed,
} = source;

export const one = 1,
  two = 2;

@decorate
export class Decorated {
  @track
  field = 1;

  @track
  method() {}
}

export async function* gen() {
  yield first;
}

export const arrow = () => {
  return renamed;
};
