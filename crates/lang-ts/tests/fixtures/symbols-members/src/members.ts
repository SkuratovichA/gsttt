import { helper, other } from './helper';

class Keys {
  [Symbol.iterator]() {
    return helper();
  }
  ['literal']() {
    return other();
  }
  plain() {}
}

class Nested {
  m() {
    function inner() {
      return this.x();
    }
    const arrow = () => this.y();
    return [inner, arrow];
  }
  x() {}
  y() {}
}

class Privates {
  #secret = 1;
  #hidden() {}
  reveal() {
    this.#hidden();
    return #secret in this;
  }
  static make(instance: Privates) {
    return instance.#secret;
  }
}

class Statics {
  static count = 0;
  static {
    helper();
  }
  static bump() {
    return Statics.count + this.count;
  }
  static [Symbol.species] = other;
}

function shadow(helper: number) {
  return helper;
}
