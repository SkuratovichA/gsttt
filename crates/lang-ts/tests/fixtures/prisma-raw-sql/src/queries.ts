import { PrismaClient } from '@prisma/client';

const prisma = new PrismaClient();

export const one = (id: string) => prisma.$queryRaw`SELECT id FROM a WHERE id = ${id}`;

export const many = (ids: string[], limit: number) =>
  prisma.$queryRaw`SELECT id FROM b WHERE id IN (${ids[0]}, ${ids[1]}) LIMIT ${limit}`;

export const typed = (id: string) =>
  prisma.$queryRaw<{ id: string }[]>`SELECT id FROM c WHERE id = ${id}::uuid`;

export const purge = () => prisma.$executeRaw`DELETE FROM d`;
