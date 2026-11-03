const lib = require('./lib');

function take() {
  const { a } = lib;
  return a;
}

module.exports = { take };
