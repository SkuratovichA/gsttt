-- sqlx: the SQL of a query is an embedded `sql` document when it is a literal. `query_as!` takes the row type first, so
-- its SQL is the second argument. The `query_file!` macros name a file instead, which the connector reads as an include.
return {
  name = "sqlx",
  language = "rust",
  detect = { "sqlx" },
  embeds = {
    {
      callee = {
        "query!", "query_unchecked!", "query_scalar!", "query_scalar_unchecked!",
        "sqlx.query!", "sqlx.query_unchecked!", "sqlx.query_scalar!", "sqlx.query_scalar_unchecked!",
      },
      lang = "sql",
      text = "$arg1",
    },
    {
      callee = { "query_as!", "query_as_unchecked!", "sqlx.query_as!", "sqlx.query_as_unchecked!" },
      lang = "sql",
      text = "$arg2",
    },
    {
      callee = { "query", "query_scalar", "query_as", "sqlx.query", "sqlx.query_scalar", "sqlx.query_as" },
      lang = "sql",
      text = "$arg1",
    },
  },
}
