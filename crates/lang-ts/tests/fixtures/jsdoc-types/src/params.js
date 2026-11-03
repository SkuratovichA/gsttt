/**
 * @import { Sel } from "./sel.js";
 */

/**
 * @param {Sel} a
 * @param {?Sel} b
 */
function compareSpecificity(a, b) {
  return a.compare(b);
}

/** @param {Sel} [a=null] */
export function optional(a) {
  return a.compare(1);
}

/** @param {Sel|undefined} a */
const arrow = (a) => a.compare(2);

/** @param {Sel} a */
exports.assigned = function (a) {
  return a.compare(3);
};

function undocumented(a) {
  return a.compare(4);
}

/**
 * @param {Sel} a
 */
function shadowed(a) {
  const run = (a) => a.compare(5);
  return run(1);
}

/**
 * @param {Sel} a
 */
function defaulted(b = (a) => a.compare(6)) {
  return b;
}
