const local = require('./local');
const { c } = require('./c');
const dep = () => 1;
const other = () => 2;
function a() {}

module.exports = {
  run() {
    return dep();
  },
  stop: function () {
    return other();
  },
  limit: 5,
  Bar: class {
    go() {
      return dep();
    }
  },
  a,
  b: local,
  c,
  d: require('./d'),
  ['computed']() {
    return dep();
  },
  ...local,
};
