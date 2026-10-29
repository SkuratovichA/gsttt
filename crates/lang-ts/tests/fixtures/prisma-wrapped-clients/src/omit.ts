import type { PrismaClient } from '@prisma/client';

export function load(client: Omit<PrismaClient, 'booking'>) {
  return client.tenant.findFirst();
}
