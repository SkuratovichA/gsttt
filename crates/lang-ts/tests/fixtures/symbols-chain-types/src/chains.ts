interface AsType { depth: number }
interface SatisfiesType { depth: number }
interface AssertionType { depth: number }
interface InstantiatedType { depth: number }
interface OuterType { depth: number }
interface InnerType { depth: number }
interface NonNullType { depth: number }
interface DeepType { a: { b: { c: number } } }
interface RootType { depth: number }

declare const root: { depth: number };
declare function make<T>(): T;

export function viaAs(o: object) {
  return (o as AsType).depth;
}

export function viaSatisfies(o: object) {
  return (o satisfies SatisfiesType).depth;
}

export function viaAssertion(o: object) {
  return (<AssertionType>o).depth;
}

export function viaInstantiation() {
  return (make<InstantiatedType>).name;
}

export function viaBoth(o: object) {
  return ((o as OuterType) satisfies InnerType).depth;
}

export function viaNonNull(o: object) {
  return (o as NonNullType)!.depth;
}

export function viaLongChain(o: object) {
  return (o as DeepType).a.b.c;
}

export function viaRoot() {
  return (root as RootType).depth;
}
