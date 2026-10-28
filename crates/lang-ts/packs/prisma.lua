-- Prisma: a model method called on the client names the model's table.
return {
  name = "prisma",
  language = "ts",
  detect = { "@prisma/client" },
  chains = {
    {
      root = { "prisma", "this.prisma", "this.prismaService", "ctx.prisma", "ctx.db" },
      kind = "table",
      role = "calls",
      segments = 1,
      terminals = {
        "findMany", "findFirst", "findFirstOrThrow", "findUnique", "findUniqueOrThrow", "create", "createMany",
        "createManyAndReturn", "update", "updateMany", "upsert", "delete", "deleteMany", "count", "aggregate",
        "groupBy",
      },
    },
  },
}
