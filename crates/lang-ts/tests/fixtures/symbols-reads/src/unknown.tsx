declare const x: any;
declare const jest: any;
declare const a: any;

export function reads(): void {
  x.a;
  x?.b;
  x['c'];
  const { d } = x;
  let e: unknown;
  ({ e } = x);
  jest.spyOn(x, 'h');
  a.j.k;
}

export function f({ g }: { g: number }): number {
  return g;
}

export const view = <x.i />;
