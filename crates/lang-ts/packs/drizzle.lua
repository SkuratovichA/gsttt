-- Drizzle: a table builder declares the table it is given the name of.
return {
  name = "drizzle",
  language = "ts",
  detect = { "drizzle-orm" },
  calls = {
    {
      callee = { "pgTable", "mysqlTable", "sqliteTable", "singlestoreTable" },
      kind = "table",
      role = "declares",
      key = "$arg1",
    },
  },
}
