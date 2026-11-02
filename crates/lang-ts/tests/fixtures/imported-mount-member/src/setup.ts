import { router } from './router';
import { server } from './server';

server.app.use('/api', router);
