import type { Prisma } from '@prisma/client';

type PrismaClient = Pick<Prisma.TransactionClient, 'tenant'>;

export function find(client: PrismaClient) {
  return client.tenant.findFirst();
}
