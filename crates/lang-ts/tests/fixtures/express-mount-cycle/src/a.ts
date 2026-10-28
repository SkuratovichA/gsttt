import { Router } from 'express';
import { b } from './b';

export const a = Router();
a.use('/b', b);
a.get('/x', (req, res) => res.json({}));
