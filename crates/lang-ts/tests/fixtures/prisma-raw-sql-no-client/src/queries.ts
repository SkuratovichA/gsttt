declare const prisma: { $queryRaw: (parts: TemplateStringsArray) => unknown };

export const one = () => prisma.$queryRaw`SELECT id FROM a`;
