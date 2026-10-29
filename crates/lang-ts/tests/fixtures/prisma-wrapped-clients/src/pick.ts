import type { PrismaClient } from '@prisma/client';

export function load(client: Pick<PrismaClient, 'tenant'>) {
  return client.tenant.findFirst();
}
