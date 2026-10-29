import { Router } from 'express';

export * from './a';

export const users = Router();
users.get('/:id', (req, res) => res.json({}));
