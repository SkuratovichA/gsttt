import { PrismaClient } from '@prisma/client';

const prisma = new PrismaClient();

const TEXT = 'SELECT id FROM f';

export const constant = () => prisma.$queryRawUnsafe(TEXT);
