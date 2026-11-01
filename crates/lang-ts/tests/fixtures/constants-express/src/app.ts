import express from 'express';
import router from './users';

const app = express();
app.use('/v1', router);

export default app;
