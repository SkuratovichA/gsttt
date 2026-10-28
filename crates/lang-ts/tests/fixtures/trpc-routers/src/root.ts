import { bookingRouter } from './booking';
import { router } from './trpc';

export const appRouter = router({
  booking: bookingRouter,
});
