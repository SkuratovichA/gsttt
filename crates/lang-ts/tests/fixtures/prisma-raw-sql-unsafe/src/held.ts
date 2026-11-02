import { PrismaClient, Prisma } from '@prisma/client';

const prisma = new PrismaClient();

const query = Prisma.sql`SELECT id FROM e`;

export const held = () => prisma.$queryRaw(query);
