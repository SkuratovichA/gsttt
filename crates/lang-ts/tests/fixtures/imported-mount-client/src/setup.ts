import { client } from './client';
import { router } from './router';

client.use('/api', router);
client.use(`${process.env.BASE}/api`, router);
