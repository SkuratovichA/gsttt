import { app } from './app';
import { router } from './router';

app.use(`${process.env.BASE}/api`, router);
