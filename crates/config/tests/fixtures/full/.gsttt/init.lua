local g = require("gsttt")

g.setup { exclude = { "fixtures/**" }, http_prefixes = { "/api" } }
g.ts { tsconfig = "tsconfig.base.json" }
g.disable "openapi"
g.disable_pack "axios"
g.external { name = "kotlin", cmd = { "gsttt-kotlin" }, files = { "**/*.kt" }, version = "1" }
