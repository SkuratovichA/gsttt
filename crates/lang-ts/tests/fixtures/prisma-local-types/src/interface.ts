interface PrismaClient {
  tenant: { findFirst(): void };
}

export function find(client: PrismaClient) {
  return client.tenant.findFirst();
}
