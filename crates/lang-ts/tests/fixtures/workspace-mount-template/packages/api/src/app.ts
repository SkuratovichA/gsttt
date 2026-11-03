import express from 'express';
import { API_PREFIX } from '@acme/shared';
import { e } from './e';

const app = express();
app.use(`${API_PREFIX}/two`, e);
