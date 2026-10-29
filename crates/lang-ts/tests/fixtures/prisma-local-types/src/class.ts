class PrismaService {
  tenant = { findFirst: () => null };
}

export function find(service: PrismaService) {
  return service.tenant.findFirst();
}
