import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import { readFileSync } from 'node:fs';
import { join } from 'path';
import leftPad from 'left-pad';
import { missing } from './nope';

export const all = [useState, createRoot, readFileSync, join, leftPad, missing];
