import type { PrismaClient } from '@prisma/client';

export class Reads {
  private client: PrismaClient;

  find() {
    return this.client.tenant.findFirst();
  }
}

export class Fakes {
  private client: { tenant: { findFirst(): void } };

  find() {
    return this.client.tenant.findFirst();
  }
}
