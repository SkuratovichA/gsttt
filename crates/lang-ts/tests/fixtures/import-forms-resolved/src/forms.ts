import { a } from './present';
export { b } from './present';
export * from './present';
const lazy = import('./present');
const cjs = require('./present');
import legacy = require('./present');
export const all = [a, lazy, cjs, legacy];
