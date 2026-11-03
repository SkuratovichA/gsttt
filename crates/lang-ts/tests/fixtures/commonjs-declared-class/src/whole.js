const dep = () => 1;

module.exports = class Foo {
  go() {
    return dep();
  }
};
