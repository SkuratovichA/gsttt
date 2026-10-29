import { PrismaClient } from '@prisma/client';

export class Repository {
  private readonly store = new PrismaClient();

  find() {
    return this.store.invoice.findFirst();
  }
}

export function total(reader = new PrismaClient()) {
  return reader.payment.count();
}
