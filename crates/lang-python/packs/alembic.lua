-- Alembic migrations: the table an `op` call names is declared by the revision. `create_index` names its table second.
-- `op` is written as it is imported, so `from alembic import op as o` is `alembic.op` and both spellings are named.
return {
  name = "alembic",
  language = "python",
  detect = { "alembic" },
  calls = {
    {
      callee = {
        "op.create_table",
        "alembic.op.create_table",
        "op.drop_table",
        "alembic.op.drop_table",
        "op.rename_table",
        "alembic.op.rename_table",
        "op.add_column",
        "alembic.op.add_column",
        "op.drop_column",
        "alembic.op.drop_column",
        "op.alter_column",
        "alembic.op.alter_column",
      },
      kind = "table",
      role = "declares",
      key = "$arg1",
    },
    {
      callee = { "op.create_index", "alembic.op.create_index" },
      kind = "table",
      role = "declares",
      key = "$arg2",
    },
  },
}
