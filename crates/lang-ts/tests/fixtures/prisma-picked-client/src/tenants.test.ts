const mockPrisma = { tenant: { findFirst: vi.fn() } };

mockPrisma.tenant.findFirst();
