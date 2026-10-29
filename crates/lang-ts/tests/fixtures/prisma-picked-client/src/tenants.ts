import type { Prisma } from '@prisma/client';

export async function load(reader: Pick<Prisma.TransactionClient, 'tenant' | 'tenantBranch'>, id: number) {
  const tenant = await reader.tenant.findUnique({ where: { id } });
  const branches = await reader.tenantBranch.findMany({ where: { tenantId: id } });
  return { tenant, branches };
}

export function pick(options: { tenant: { findFirst(): void } }) {
  return options.tenant.findFirst();
}
