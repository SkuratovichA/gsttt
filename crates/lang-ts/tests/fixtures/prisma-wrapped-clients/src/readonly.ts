import type { PrismaClient } from '@prisma/client';

export function load(client: Readonly<PrismaClient>) {
  return client.tenant.findFirst();
}
