import { Service, FixTracker, makeService, logger } from './service';
import * as lib from './lib';

export function viaConstant() {
  const s = Service.make();
  s.place();
}

export function viaInstance() {
  Service.getInstance().place();
}

export async function viaAwait() {
  const s = await Service.create();
  s.place();
}

export function viaParentheses() {
  return (Service.make()).place();
}

export function viaChain(a: number, b: number, c: number) {
  new FixTracker(a).retain(b).remove(c);
}

export function viaStaticChain() {
  Service.make().with(1).place();
}

export function viaConstantChain(a: number, b: number, c: number) {
  const t = new FixTracker(a);
  t.retain(b).remove(c);
}

export function viaLowerCase() {
  logger.child().info();
}

export function viaNamespace() {
  lib.make().place();
}

export function viaFunction() {
  makeService().place();
}
