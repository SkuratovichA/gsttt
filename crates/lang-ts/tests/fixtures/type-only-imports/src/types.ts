import type { A } from './a';
import { type B, C } from './b';

export type Both = [A, B, typeof C];
