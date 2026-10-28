/// <reference path="./globals.d.ts" />
import def, { a, b as c } from './named';
import * as ns from './namespace';
import './side-effect';
import type { T } from './types';
export { x, y as z } from './re-exported';
export * from './all';
export * as star from './star';
const lazy = import('./lazy');
const cjs = require('./cjs');
import legacy = require('./legacy');

export type Kept = T;
export const used = [def, a, c, ns, lazy, cjs, legacy];
import { ok } from './present';
