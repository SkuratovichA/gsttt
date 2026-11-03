import { Router } from 'express';

export const users = Router();
users.get('/:id', (req, res) => res.json({}));

const fallback = Router();
fallback.get('/legacy', (req, res) => res.json({}));

export default fallback;
