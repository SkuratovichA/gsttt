import { Inject, a, b, c, d, e, f, g, h, i, j, k, l, m, n } from './deps';
import type { Dep } from './dep';

enum Level {
  Low = a(),
  High = b,
}

namespace Space {
  c();
  export const inner = d;
  export function later() {
    e();
  }
}

const iife = (() => f())();
const tagged = g`x ${h}`;
const made = new i(j);
const lazy = () => k();
const object = {
  method() {
    l();
  },
  value: m(),
};

class Service {
  constructor(@Inject(n) private readonly dep: Dep) {}
}
