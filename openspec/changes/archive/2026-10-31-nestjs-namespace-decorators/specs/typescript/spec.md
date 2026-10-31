# Spec Delta

## MODIFIED Requirements

### Requirement: NestJS routes
With the `nestjs` pack, `@Get`, `@Post`, `@Put`, `@Patch`, `@Delete`, `@Options`, `@Head` and `@All` on a method of a `@Controller('<prefix>')` class SHALL serve the prefix and the path joined, also when they are called through a namespace import (`@common.Get`).

#### Scenario: A NestJS controller serves its routes
- **WHEN** a class decorated `@Controller('bookings')` has a method decorated `@Get(':id')`
- **THEN** the file serves `http GET /bookings/{}` at the line of `@Get`

#### Scenario: A NestJS controller imported as a namespace serves its routes
- **WHEN** a file has `import * as common from '@nestjs/common'` and a class decorated `@common.Controller('users')` has methods decorated `@common.Get('/:id')` and `@common.Post()`
- **THEN** the file serves `http GET /users/{}` and `http POST /users` at the lines of the decorators

### Requirement: GraphQL
With the `nestjs` pack, `@Query`, `@Mutation` and `@Subscription` on a method SHALL serve `graphql <Root>.<name>`, the name taken from the decorator's `name` option or else the method's name, also when they are called through a namespace import (`@graphql.Query`).

#### Scenario: A resolver method serves a root field
- **WHEN** a `@Resolver()` class has `@Query(() => [Booking]) bookings()` and `@Mutation(() => Booking, { name: 'cancelBooking' }) cancel()`
- **THEN** the file serves `graphql Query.bookings` and `graphql Mutation.cancelBooking`

#### Scenario: A resolver imported as a namespace serves its root fields
- **WHEN** a file has `import * as graphql from '@nestjs/graphql'` and a `@graphql.Resolver()` class has `@graphql.Query(() => [User]) users()`, `@graphql.Mutation(() => User, { name: 'removeUser' }) remove()` and `@graphql.ResolveField(() => Org) org()`
- **THEN** the file serves `graphql Query.users` and `graphql Mutation.removeUser` and nothing for `org`
