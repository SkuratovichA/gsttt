const lib = require('./a');
const { b, c: d } = require('./a');

function run() {
  return lib.x + b + d;
}
