const prismaBlueprintFindManyMock = () => [];
const prismaMock = {
  blueprint: { findMany: prismaBlueprintFindManyMock },
  resource: { findMany: prismaBlueprintFindManyMock },
};
prismaMock.resource.findMany.mockReturnValueOnce([]);
const prismaServiceMock = { resource: { findMany: async () => [1] } };
prismaServiceMock.resource.findMany.mockResolvedValueOnce([]);
export const mocks = { prismaMock, prismaServiceMock };
