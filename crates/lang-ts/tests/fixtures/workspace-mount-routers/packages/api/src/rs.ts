import { Router } from 'express';

export const ra = Router();
ra.get('/ra', (req, res) => res.end());

export const rb = Router();
rb.get('/rb', (req, res) => res.end());
