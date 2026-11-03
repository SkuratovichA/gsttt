const Service = require('./service');
const lib = require('./lib');

function viaRequire() {
  const s = Service.make();
  s.place();
}

function viaRequiredLower() {
  lib.make().place();
}

const shared = Service.make();
const fromLib = lib.make();
module.exports = { viaRequire, viaRequiredLower, shared, fromLib };
