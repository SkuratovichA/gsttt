import type { Prisma } from '@prisma/client';

export function a(tx2: Pick<Prisma.TransactionClient, 'tenant'>) {
  return tx2.tenant.findFirst();
}

export function b(tx2: { tenant: { findFirst(): void } }) {
  return tx2.tenant.findFirst();
}
