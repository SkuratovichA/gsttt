import express from 'express';
import { shared } from '@acme/routes';

const app = express();
app.use('/ext', shared);
