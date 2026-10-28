import type { PrismaClient } from '@prisma/client';

export function load(client: PrismaClient | null) {
  return client.tenant.findFirst();
}
