const a = {} as Service;
a.place();
const b = <Service>{};
b.other();
const c = { d: 1 } satisfies Service;
c.third();
