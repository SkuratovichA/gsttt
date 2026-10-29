import { PrismaClient } from '@prisma/client';
import { PrismaService } from './prisma.service';

export class InvoiceRepo {
  private readonly client: PrismaClient;

  constructor(private readonly store: PrismaService, plain: PrismaService) {
    this.client = plain;
  }

  all() {
    return this.store.invoice.findMany();
  }

  one(id: number) {
    return this.client.payment.findUnique({ where: { id } });
  }

  none() {
    return this.plain.refund.findMany();
  }
}

export async function count(client: PrismaClient | null) {
  return client.booking.count();
}
