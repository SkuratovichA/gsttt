import express from 'express';
import { outer } from '@acme/routes';

const app = express();
app.use('/ext', outer);
