import { app } from './app';
import { router } from './router';

type Server = { use(path: string, handler: unknown): void };

export function setup(app: Server) {
  app.use('/api', router);
}
