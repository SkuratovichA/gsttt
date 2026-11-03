export class Holder {
  constructor(private readonly ctx: any) {}
  #cache: any;

  deep(): void {
    this.ctx.logger.error();
  }

  hidden(): void {
    this.#cache.size.valueOf();
  }
}

export function param(ctx: any): void {
  ctx.logger.error();
}

export function sees(x: any): void {
  x.a!.b;
  x.c?.d;
  x.e?.f!.g;
}

export function afterCall(x: any): void {
  x.get().name;
  x?.get()?.other;
}

export function afterAwait(x: any): Promise<void> {
  return (async () => {
    (await x.load()).title;
  })();
}

export function builtin(list: string[]): void {
  Object.keys(list).length;
  JSON.parse('{}').a.b;
  process.env.HOME;
}
