const A = '/a' as const;
const B = `${A}/b`;
const C = A + '/c';
const D = ('/d' satisfies string);
export enum Paths {
  E = '/e',
  N = 3,
}
export class Cls {
  static G = '/g';
  H = '/h';
}
const OBJ = { i: '/i', nested: { j: '/j' }, ['k']: '/k', 'l-x': '/l', ...Cls };
let later = '/z';

fetch(B);
fetch(C);
fetch(D);
fetch(Paths.E);
fetch(Paths.N);
fetch(Cls.G);
fetch(Cls.H);
fetch(OBJ.i);
fetch(OBJ.nested.j);
fetch(OBJ.k);
fetch(OBJ['l-x']);
fetch(later);
fetch(`${B}/${later}`);
