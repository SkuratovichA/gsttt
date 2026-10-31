import * as graphql from '@nestjs/graphql';

class User {}
class Org {}

@graphql.Resolver()
export class UsersResolver {
  @graphql.Query(() => [User])
  users() {}

  @graphql.Mutation(() => User, { name: 'removeUser' })
  remove() {}

  @graphql.ResolveField(() => Org)
  org() {}
}
