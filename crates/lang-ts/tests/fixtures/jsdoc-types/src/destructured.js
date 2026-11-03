/**
 * @import { Linter, ConfigLoader } from "./linter.js";
 */

/**
 * @param {Object} config
 * @param {Linter} config.linter
 * @param {ConfigLoader} config.configLoader
 */
function run({ text, linter, configLoader }) {
  linter.verifyAndFix(text);
  return configLoader.loadConfigArrayForDirectory(text);
}

/**
 * @param {Linter} config.linter
 */
function chained(config) {
  return config.linter.getFixPassCount();
}
