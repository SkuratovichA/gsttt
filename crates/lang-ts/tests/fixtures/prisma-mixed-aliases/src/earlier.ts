import { PrismaClient } from '@prisma/client';

export function other(self) {
  return self.prisma.tenant.findMany();
}

export class TenantService {
  prisma = new PrismaClient();

  find() {
    const self = this;
    return self.prisma.tenant.findFirst();
  }
}
