import type { PrismaClient } from '@prisma/client';

export function plain(tx2) {
  return tx2.tenant.findFirst();
}

export function typed(tx2: PrismaClient) {
  return tx2.tenant.findFirst();
}

export class Earlier {
  client = makeClient();

  find() {
    return this.client.tenant.findFirst();
  }
}

export class Later {
  private client: PrismaClient;

  find() {
    return this.client.tenant.findFirst();
  }
}
