-- TypeORM: an entity declares its table, named by the decorator or else by the class.
return {
  name = "typeorm",
  language = "ts",
  detect = { "typeorm", "@nestjs/typeorm" },
  decorators = {
    { decorator = "Entity", kind = "table", role = "declares", key = "$arg1|$arg1.name|$member" },
  },
}
