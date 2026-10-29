import { PrismaClient } from '@prisma/client';

const client = new PrismaClient({ log: ['query'] });

export function count() {
  return client.booking.count();
}
