import { Base, Service, Repository, mixin } from './base';
import * as lib from './lib';

class Plain extends Base<string> {}
class Qualified extends lib.Base {}
class Mixed extends mixin(Base) {}
class Alone {}
class Promised implements Alone {}

interface Wide extends Base, lib.Shape {}
export default interface Named extends Base {}

export const made = new Service();
export const typed: Service = make();
export const generic = new Repository<string>();
export const unknown = make();
let later = new Service();
var oldest = new Service();
function inner() {
  const nested = new Service();
}
