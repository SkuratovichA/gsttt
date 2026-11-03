const helper = require('./helper');
const dep = () => 1;

exports.run = function () {
  return dep() + helper.go();
};

exports.stop = () => {
  return dep();
};

exports.a = exports.b = function () {
  return dep();
};

module.exports.limit = 5;

exports.label = compute(() => dep());

exports.local = dep;
exports.loaded = require('./other');

function compute(f) {
  return f();
}

exports.again = function () {
  return exports.run();
};
