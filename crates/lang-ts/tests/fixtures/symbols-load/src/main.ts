import { D, f, g, h, k, mixin, n } from './x';
import { test } from 'vitest';

const a = f();
const b = g;

test('x', () => h());

class C extends mixin(D) {
  static s = k();
  m() {
    n();
  }
}
