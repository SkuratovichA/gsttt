import express from 'express';
import { LEGACY_PATH } from './paths';

const app = express();
app.get(['/a', LEGACY_PATH], (req, res) => res.send('ok'));

export default app;
