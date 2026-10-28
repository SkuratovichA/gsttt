import { publicProcedure, router } from './server';
import { other } from './other';

const list = publicProcedure.query(() => []);
const remove = publicProcedure.mutation(() => true);

export const appRouter = router({
  list,
  remove,
  ...other,
  nested: other.router,
  inline: publicProcedure.query(() => 1),
});
