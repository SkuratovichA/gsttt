const { check, helper } = require('./member');
function a() {
  return check();
}
function b() {
  return helper();
}
module.exports = { a, b };
