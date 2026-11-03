/**
 * @import { Visitor } from "./visitor.js";
 */

class Traverser {
  /**
   * @param {Visitor} visitor
   */
  static async run(visitor) {
    visitor.callSync(1);
  }

  /** @param {Visitor} visitor */
  each(visitor) {
    visitor.callSync(2);
  }

  skip(visitor) {
    visitor.callSync(3);
  }
}

const table = {
  /** @param {Visitor} v */
  visit(v) {
    v.callSync(4);
  },
};

module.exports = { Traverser, table };
