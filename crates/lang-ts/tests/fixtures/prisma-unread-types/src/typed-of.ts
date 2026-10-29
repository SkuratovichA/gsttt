import { PrismaClient } from '@prisma/client';

const shared = new PrismaClient();

export function find(client: typeof shared) {
  return client.tenant.findFirst();
}
