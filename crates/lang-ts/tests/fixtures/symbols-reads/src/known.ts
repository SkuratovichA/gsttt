import { Repo } from './repo';
import { Service } from './service';

export class Base {
  up(): void {}
}

export class Holder extends Base {
  constructor(private repo: Repo) {
    super();
  }

  own(): void {}

  run(): void {
    this.own();
    super.up();
    this.repo.save();
    const y = new Service();
    y.known();
    const key = { key: 1 };
    const k = 'key';
    const z: any = key;
    z[k];
  }
}

export type P = Service['place'];
