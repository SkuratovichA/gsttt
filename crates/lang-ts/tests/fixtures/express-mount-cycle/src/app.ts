import express from 'express';
import { a } from './a';

const app = express();
app.use('/api', a);
