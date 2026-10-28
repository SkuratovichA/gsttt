import { Router } from 'express';

export const extras = Router();
extras.get('/health', (req, res) => res.json({}));
