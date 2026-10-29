import express from 'express';
import { gone } from './a';

const app = express();
app.use('/gone', gone);
