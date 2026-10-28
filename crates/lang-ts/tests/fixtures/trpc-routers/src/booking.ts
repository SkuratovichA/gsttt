import { publicProcedure, router } from './trpc';

declare const s: never;
declare function f(): string;

export const bookingRouter = router({
  get: publicProcedure.input(s).query(f),
});
