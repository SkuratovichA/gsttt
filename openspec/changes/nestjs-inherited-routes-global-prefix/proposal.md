# Proposal: NestJS routes inherited from a base class and behind the global prefix

## Why

A NestJS monorepo whose controllers come from a code generator (Amplication) uses two shapes the TypeScript connector did not read. A generated controller is `@Controller('users') class UserController extends UserControllerBase {}`, and every route is a method of `UserControllerBase` in another file, which carries no `@Controller`, so the routes were never served. And `app.setGlobalPrefix('api')` in `main.ts` puts every controller that `AppModule` reaches behind `/api`, so an ordinary `@Controller()` class with `@Post('login')` was keyed `http POST /login`, a route that does not exist. A route keyed at the wrong path is a wrong answer that looks complete, and a route that is missing makes a change to its handler look as if nothing calls it.

## What Changes

- A route decorator on a method of a class that has no `@Controller` decorator is kept for the link phase. A controller that extends the class by name, which may be imported from another file, serves those routes under its own prefix, at the line of the method in the file of the base class and at the line of its `@Controller` decorator. A base class that nothing extends serves nothing. Only the class a controller extends directly is read, not a base of that class.
- `@Module({ imports, controllers })` classes mount the modules and controllers they list as plain names or `forwardRef(() => Name)`. `app.setGlobalPrefix('<literal>')` on an app that `NestFactory.create(<Root>)` made in the same file mounts `<Root>` under the prefix. The link phase walks these mounts with the walk it already has for Express routers, so a controller is served under every prefix of the apps that reach it, the routes it inherits included, and keeps its path when no app with a prefix reaches it. A prefix that is not a literal is an unread site.
- A variable made by `await call(…)` is a maker like one made by `call(…)`, and a maker keeps its first argument when that is a plain name.
- The blind spot "a global prefix set at start-up (NestJS setGlobalPrefix) is not applied" is replaced by what is still not followed, and a blind spot for inheritance over more than one level is added.
- The version of the reader goes from 2 to 3, because the facts and the data kept for the link phase change.

## Impact

- Capability `typescript`.
- `crates/lang-ts/` only: the analyzer, `TsExtra`, the link phase, the blind spots, and fixtures and tests.
- Indexes built by an earlier version are parsed again once.
