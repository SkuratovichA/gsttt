import { PrismaClient, Prisma } from '@prisma/client';

const prisma = new PrismaClient();

export const literal = () => prisma.$queryRawUnsafe('SELECT id FROM a');

export const run = () => prisma.$executeRawUnsafe(`UPDATE b SET seen = true`);

export const computed = (table: string) => prisma.$queryRawUnsafe(`SELECT id FROM ${table}`);

export const wrapped = (id: string) =>
  prisma.$queryRaw(Prisma.sql`SELECT id FROM c WHERE id = ${id}`);

export const variable = (text: string) => prisma.$executeRawUnsafe(text);
