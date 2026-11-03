const dep = () => 1;

exports.Foo = class Inner {
  run() {
    return dep();
  }
};
