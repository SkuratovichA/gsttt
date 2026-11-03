import { Router } from 'express';
import { inner } from './inner';

export const outer = Router();
outer.use('/inner', inner);
