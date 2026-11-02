import { PrismaClient } from '@prisma/client';

const prisma = new PrismaClient();

prisma.tenant.findFirst = vi.fn();

export const all = () => prisma.booking.findMany();
