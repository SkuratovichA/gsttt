-- Environment variables: `os.environ["A"]`, `os.environ.get("A")`, `os.getenv("A")` and each field of a pydantic
-- settings class, named after the field (behind the class's `env_prefix`) in upper case. Always on.
return {
  name = "env",
  language = "python",
  calls = {
    {
      callee = { "os.environ.get", "environ.get", "os.getenv", "getenv" },
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
      class = { "BaseSettings", "*.BaseSettings" },
      kind = "env",
      role = "calls",
      key = "$callee",
    },
  },
}
