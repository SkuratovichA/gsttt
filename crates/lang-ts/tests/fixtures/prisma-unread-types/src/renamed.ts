import type { PrismaClient as Base } from '@prisma/client';

export function find(client: Base) {
  return client.tenant.findFirst();
}
