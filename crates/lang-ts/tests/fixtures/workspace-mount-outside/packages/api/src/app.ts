import express from 'express';
import { OUTSIDE } from '@outside/prefix';
import { e } from './e';

const app = express();
app.use(`${OUTSIDE}/two`, e);
