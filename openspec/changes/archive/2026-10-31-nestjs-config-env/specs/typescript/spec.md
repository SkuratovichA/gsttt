# Spec Delta

## ADDED Requirements

### Requirement: NestJS configuration
With the `nestjs-config` pack, `get` and `getOrThrow` on a `configService` receiver SHALL call `env <NAME>` when the first argument is a string that names a variable; a first argument that is a path into a loaded configuration, such as `database.host`, SHALL name no environment variable, and a computed first argument SHALL be a blind spot.

#### Scenario: A ConfigService read names an environment variable
- **WHEN** a package declares `@nestjs/config` and a file calls `this.configService.get<string>('API_URL', 'http://localhost')` and `configService.getOrThrow('DB_URL')`
- **THEN** the file calls `env API_URL` and `env DB_URL`

#### Scenario: A configuration path is no environment variable
- **WHEN** a file calls `this.configService.get('database.host')`
- **THEN** it calls no `env` key at that line

#### Scenario: A computed configuration name is a blind spot
- **WHEN** a file calls `this.configService.get(Env.API_URL)`, where `Env` is an imported enum
- **THEN** the file has an unread site at that line
