import { PrismaClient } from '@prisma/client';

const prisma = new PrismaClient();
const client = makeCache();

export async function users() {
  await prisma.$transaction(async (client) => {
    await client.user.findMany();
  });
  return client.user.findMany();
}
