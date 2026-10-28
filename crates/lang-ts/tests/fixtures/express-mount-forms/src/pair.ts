import { Router } from 'express';

export const pair = Router();
pair.get('/z', (req, res) => res.json({}));
