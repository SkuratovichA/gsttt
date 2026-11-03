import { Logger, Store, Other } from './lib';

export class Field {
  private logger: Logger;
  built = new Store();
  maybe: Logger | null = null;
  #secret: Logger;
  untyped = 1;
  unionOf: Logger | Other;
  primitive: string;
  own: Field;
  stamp: Date;
  list: Array<Logger>;

  constructor(
    private readonly injected: Logger,
    plain: Other,
    public named: Store,
  ) {}

  get current(): Logger {
    return this.logger;
  }

  fetch(): Promise<Store> {
    return Promise.resolve(new Store());
  }

  either(): Logger | Other {
    return this.logger;
  }

  count(): number {
    return 1;
  }

  unknown() {
    return this.logger;
  }
}

export interface Shape {
  logger: Logger;
  optional?: Store;
  both: Logger | Other;
  size: number;
  load(): Store;
  later(): Promise<Logger>;
  get view(): Logger;
}

export type Literal = {
  logger: Logger;
  count: number;
};

export type Alias = Logger;
