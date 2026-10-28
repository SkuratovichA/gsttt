import { Router } from 'express';

export const users = Router();
users.get('/list', (req, res) => res.json([]));
