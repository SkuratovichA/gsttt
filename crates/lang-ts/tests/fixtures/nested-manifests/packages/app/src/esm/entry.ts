import { createElement } from 'react';
import { chunk } from 'lodash';
import { v4 } from 'uuid';
import { pad } from 'left-pad';

export const entry = [createElement, chunk, v4, pad];
