import { Router } from 'express';
import { a } from './a';

export const b = Router();
b.use('/a', a);
b.get('/y', (req, res) => res.json({}));
