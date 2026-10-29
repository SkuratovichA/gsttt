import type { PrismaClient } from '@prisma/client';

export function load(client: NonNullable<PrismaClient>) {
  return client.tenant.findFirst();
}
