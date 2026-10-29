import type { PrismaClient } from '@prisma/client';

export function load(client: Required<PrismaClient>) {
  return client.tenant.findFirst();
}
