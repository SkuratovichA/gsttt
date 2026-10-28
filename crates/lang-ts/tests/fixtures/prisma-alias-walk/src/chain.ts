import type { PrismaClient } from '@prisma/client';

type Chain1 = PrismaClient;
type Chain2 = Chain1;
type Chain3 = Chain2;
type Chain4 = Chain3;
type Chain5 = Chain4;
type Chain6 = Chain5;
type Chain7 = Chain6;
type Chain8 = Chain7;
type Chain9 = Chain8;

export function find(client: Chain9) {
  return client.tenant.findFirst();
}
