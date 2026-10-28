-- Environment variables, which every project reads through the standard library whether or not it depends on a crate for
-- it: `env::var("A")`, `std::env::var_os("B")`, the macros `env!("C")` and `option_env!("D")`, `dotenvy::var("E")`, and
-- a clap argument that falls back to a variable: `#[arg(long, env = "F")]`.
return {
  name = "env",
  language = "rust",
  calls = {
    {
      callee = {
        "env.var", "env.var_os", "std.env.var", "std.env.var_os", "dotenvy.var",
        "env!", "std.env!", "option_env!", "std.option_env!",
      },
      kind = "env",
      role = "calls",
      key = "$arg1",
    },
  },
  decorators = {
    { decorator = { "arg", "clap" }, kind = "env", role = "calls", key = "$env" },
  },
}
