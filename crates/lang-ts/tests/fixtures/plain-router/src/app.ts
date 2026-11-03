import express, { Router } from 'express';

const app = express();
app.get('/e', (req, res) => res.send('e'));
const router = Router();
router.get('/f', (req, res) => res.send('f'));
