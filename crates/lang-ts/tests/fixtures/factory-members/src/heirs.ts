import { Service, Other } from './service';
import * as lib from './lib';

export function typed(): Service { return new Service(); }
export async function promised(): Promise<Service> { return new Service(); }
export function nullable(): Service | null { return null; }
export function untyped() { return new Service(); }
export const arrow = () => new Service();
export const awaited = async () => (await new Service());
export const expression = function () { return new Service(); };
export function deep(flag: boolean) {
  if (flag) {
    return new Service();
  }
  return null;
}
export function many(flag: boolean) {
  return flag ? null : new Other();
}
export function either(flag: boolean) {
  if (flag) return new Service();
  return new Other();
}
export function nested() {
  return () => new Service();
}
export function unbound() { return new Missing(); }

export const made = typed();
export const waited = await promised();
export const unknown = missing();
export const fromStatic = Service.make();
export const fromLower = lib.make();
