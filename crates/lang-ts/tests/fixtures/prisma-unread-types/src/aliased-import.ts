type Db = import('@prisma/client').PrismaClient;

export function find(client: Db) {
  return client.tenant.findFirst();
}
