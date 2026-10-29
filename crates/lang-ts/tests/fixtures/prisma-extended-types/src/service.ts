import { PrismaClient } from '@prisma/client';

export class PrismaService extends PrismaClient {
  async onModuleInit() {
    await this.$connect();
  }
}

export class TenantRepository {
  constructor(private readonly store: PrismaService) {}

  find() {
    return this.store.tenant.findFirst();
  }
}
