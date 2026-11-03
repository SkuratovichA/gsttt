/**
 * @import { A, B } from "./ab.js";
 * @param {A|B} union
 * @param {Foo<Bar>} generic
 * @param {sel} lower
 * @param {Object} object
 * @param {Array} array
 * @param {A[]} list
 */
function f(union, generic, lower, object, array, list) {
  union.one();
  generic.two();
  lower.three();
  object.four();
  array.five();
  list.six();
}
