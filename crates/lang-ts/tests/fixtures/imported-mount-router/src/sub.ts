import express from 'express';

export const sub = express.Router();

sub.get('/x', (req, res) => res.json([]));
