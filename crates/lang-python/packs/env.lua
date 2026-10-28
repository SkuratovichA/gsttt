-- Environment variables: `os.environ["A"]`, `os.environ.get("A")`, `os.environ.setdefault("A", …)`, `os.getenv("A")`
-- and each field of a pydantic settings class, named after the field (behind the class's `env_prefix`) in upper case,
-- or after its `env` (pydantic 1), `validation_alias` or `alias` when it has a literal one, which is the name as it
-- stands, with no prefix.
-- Always on.
--
-- Only a class that names `BaseSettings` as a base is a settings class: a class that derives from another settings
-- class is not read, which the provides say. A pattern that starts with `*` meets a base as written only, so the two
-- libraries that define `BaseSettings` are named for a class that imports it under another name.
return {
  name = "env",
  language = "python",
  calls = {
    {
      callee = {
        "os.environ.get",
        "environ.get",
        "os.environ.setdefault",
        "environ.setdefault",
        "os.getenv",
        "getenv",
      },
      kind = "env",
      role = "calls",
      key = "$arg1",
    },
  },
  chains = {
    {
      root = { "os.environ", "environ" },
      kind = "env",
      role = "calls",
      segments = 1,
    },
  },
  assigns = {
    {
      target = "*",
      class = { "BaseSettings", "*.BaseSettings", "pydantic_settings.BaseSettings", "pydantic.BaseSettings" },
      kind = "env",
      role = "calls",
      key = "$env|$validation_alias|$alias|$callee",
    },
  },
}
