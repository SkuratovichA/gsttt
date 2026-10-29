enum PrismaClient {
  Tenant,
}

export function find(client: PrismaClient) {
  return client.tenant.findFirst();
}
