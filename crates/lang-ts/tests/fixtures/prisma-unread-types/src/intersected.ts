import type { PrismaClient } from '@prisma/client';

export function find(client: PrismaClient & { extra: true }) {
  return client.tenant.findFirst();
}
