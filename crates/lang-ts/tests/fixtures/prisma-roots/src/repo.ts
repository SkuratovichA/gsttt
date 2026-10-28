import { PrismaClient } from '@prisma/client';
export const db = new PrismaClient();
export async function load(tx: PrismaClient) {
  const rows = await db.invoice.findMany();
  await tx.payment.create({ data: {} });
  return rows;
}
