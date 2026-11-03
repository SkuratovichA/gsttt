import * as lib from './lib';

class Aws extends Base implements Provider, lib.Other<string> {}

interface Wide extends Base {}

class Plain {}

export const made = new Aws();
