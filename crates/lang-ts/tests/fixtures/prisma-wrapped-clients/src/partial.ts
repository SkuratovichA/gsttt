import type { PrismaClient } from '@prisma/client';

export function load(client: Partial<PrismaClient>) {
  return client.tenant.findFirst();
}
