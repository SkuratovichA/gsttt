-- Alembic migrations: the table an `op` call names is declared by the revision. `create_index` names its table second.
return {
  name = "alembic",
  language = "python",
  detect = { "alembic" },
  calls = {
    {
      callee = {
        "op.create_table",
        "op.drop_table",
        "op.rename_table",
        "op.add_column",
        "op.drop_column",
        "op.alter_column",
      },
      kind = "table",
      role = "declares",
      key = "$arg1",
    },
    {
      callee = "op.create_index",
      kind = "table",
      role = "declares",
      key = "$arg2",
    },
  },
}
