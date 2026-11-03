import { Router } from 'express';

export const shared = Router();
shared.get('/x', (req, res) => res.end());
