function f() {}

function g(value: string): void;
function g(value: number): void;
function g(value: string | number) {
  return value;
}

const { a, b: c } = obj;

class C {
  constructor() {}
  m() {}
  static s = 1;
  #p = 2;
  get v() {
    return 1;
  }
  set v(x) {}
}

enum E {}

interface I {
  first: string;
}
interface I {
  second: number;
}

type T = string;

namespace N {}

export default 1;
