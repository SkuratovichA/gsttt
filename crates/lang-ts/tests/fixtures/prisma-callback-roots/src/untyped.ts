import { PrismaClient } from '@prisma/client';

const prisma = new PrismaClient();

export async function users() {
  return prisma.$transaction(async (client) => {
    await client.user.findMany();
  });
}
