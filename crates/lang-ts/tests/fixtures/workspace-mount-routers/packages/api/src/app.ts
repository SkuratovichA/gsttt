import express, { Router } from 'express';
import { ra, rb } from './rs';

const app = express();
const w = Router();
w.use(ra, rb);
app.use('/w', w);
