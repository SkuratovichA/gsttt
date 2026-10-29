import type { PrismaClient } from '@prisma/client';

export function first(client: PrismaClient) {
  return client.tenant.findFirst();
}

export function second(client: PrismaClient | null) {
  return client.booking.count();
}
