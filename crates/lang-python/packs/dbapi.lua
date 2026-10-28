-- DB-API SQL: the first argument of `execute` and `executemany` on any receiver, up to three segments deep
-- (`cursor.execute`, `self.cursor.execute`, `self.db.cursor.execute`). Always on: no manifest tells that a project uses a
-- DB-API driver.
return {
  name = "dbapi",
  language = "python",
  embeds = {
    {
      callee = {
        "*.execute",
        "*.executemany",
        "*.*.execute",
        "*.*.executemany",
        "*.*.*.execute",
        "*.*.*.executemany",
      },
      lang = "sql",
    },
  },
}
