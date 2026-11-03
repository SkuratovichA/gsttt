import { Router } from 'express';

export const inner = Router();
inner.get('/x', (req, res) => res.end());
