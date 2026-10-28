local g = require("gsttt")

g.setup { exclude = { "fixtures/**" }, http_prefixes = { "/api" } }
g.ts { tsconfig = "tsconfig.base.json" }
g.disable "openapi"
g.disable_pack "axios"
g.external {
  name = "kotlin",
  cmd = { "gsttt-kotlin" },
  files = { "**/*.kt" },
  version = "1",
  env = { "JAVA_HOME" },
  provides = { { kind = "http", role = "serves", via = { "ktor" }, blind_spots = { "routes built at run time" } } },
  options = { strict = true },
}
