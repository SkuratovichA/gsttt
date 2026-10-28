-- Environment variables, read from the process or from the bundler.
return {
  name = "env",
  language = "ts",
  chains = {
    { root = { "process.env", "import.meta.env" }, kind = "env", role = "calls", segments = 1 },
  },
}
