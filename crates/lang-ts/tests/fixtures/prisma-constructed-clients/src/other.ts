import type { PrismaClient } from '@prisma/client';

const client = new Map();

export function find(client: PrismaClient) {
  return client.tenant.findFirst();
}
