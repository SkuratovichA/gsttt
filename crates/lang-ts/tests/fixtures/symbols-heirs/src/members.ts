import { Base, Service } from './base';

class Sub extends Base {
  place() {
    super.place();
    this.place();
    this.own();
    super.own;
  }
  own() {}
  static make() {
    return Sub.make();
  }
  constructor(x: string) {
    super(x);
  }
}

class Headless {
  run() {
    this.go();
    super.go();
  }
}

export const orders = new Service();
export function use() {
  orders.place();
}
export function build() {
  Sub.make();
}
