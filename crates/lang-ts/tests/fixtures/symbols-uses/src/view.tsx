import { useX } from './hooks';
import type { Props } from './types';
import { Widget } from './widget';

const helper = (x: number) => x + 1;

function View(p: Props) {
  const x = useX();
  return <Widget value={helper(x)} />;
}

class C {
  a() {
    this.b();
  }
  b() {}
  static s() {}
}

function d() {
  return C.s();
}
