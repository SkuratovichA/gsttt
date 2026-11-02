import { PrismaClient } from '@prisma/client';

const prisma = new PrismaClient();

export function users() {
  return prisma.$transaction([prisma.user.findMany()]);
}
