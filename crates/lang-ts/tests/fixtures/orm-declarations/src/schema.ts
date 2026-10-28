import { pgTable, serial } from 'drizzle-orm/pg-core';

export const items = pgTable('booking_items', {
  id: serial('id').primaryKey(),
});
