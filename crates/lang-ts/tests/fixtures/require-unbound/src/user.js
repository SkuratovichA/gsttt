const { a, ...rest } = require('./a');
const { a: first = 1 } = require('./a');
const { a: { b } } = require('./a');

function run() {
  const lib = require('./a');
  return lib.a;
}
