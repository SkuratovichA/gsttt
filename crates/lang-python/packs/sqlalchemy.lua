-- SQLAlchemy tables and SQL. A declarative class names its table in `__tablename__`, a Core table in
-- `Table("name", metadata, ...)`; a string given to `text()` is a SQL document.
return {
  name = "sqlalchemy",
  language = "python",
  detect = { "sqlalchemy" },
  calls = {
    {
      callee = { "Table", "sa.Table", "sqlalchemy.Table" },
      kind = "table",
      role = "declares",
      key = "$arg1",
    },
  },
  assigns = {
    {
      target = "__tablename__",
      kind = "table",
      role = "declares",
      key = "$arg1",
    },
  },
  embeds = {
    {
      callee = { "text", "sa.text", "sqlalchemy.text" },
      lang = "sql",
    },
  },
}
