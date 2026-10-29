import { PrismaClient } from '@prisma/client';

const client = new PrismaClient();

export function seed(client: PrismaClient) {
  return client.tenant.createMany();
}
