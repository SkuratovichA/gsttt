import { use } from './lib';

declare module 'ext' {
  export function f(): typeof use;
}

declare global {
  interface Window {
    helper: typeof use;
  }
}

namespace Outer {
  export const inner = 1;
}

import Alias = Outer.inner;

export function read() {
  return Alias;
}
