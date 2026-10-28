import { Router } from 'express';

export const mixed = Router();
mixed.get('/m', (req, res) => res.json({}));
