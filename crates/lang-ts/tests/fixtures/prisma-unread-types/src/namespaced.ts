import type { Prisma as P } from '@prisma/client';

export function find(reader: P.TransactionClient) {
  return reader.tenant.findFirst();
}
