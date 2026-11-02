import { Prisma } from '@prisma/client';

export const whole = (id: string) => Prisma.sql`SELECT id FROM a WHERE id = ${id}`;

export const partial = (id: string) => Prisma.sql`AND id = ${id}`;
