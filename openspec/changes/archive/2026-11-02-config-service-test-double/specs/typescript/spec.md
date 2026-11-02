## ADDED Requirements

### Requirement: A service a test provides itself
A call rule MAY name in `test_double` an injection token. In a test file that provides a stand-in for that token, by an object with `provide` and one of `useValue`, `useFactory`, `useClass` or `useExisting`, or by `overrideProvider`, anywhere in the file, the rule SHALL read nothing: no contract and no unread site. The `nestjs-config` pack names `ConfigService`. Any other file, and a test with no stand-in, SHALL keep its reads.

#### Scenario: A read of a ConfigService a test provides is no environment read
- **WHEN** a spec file has `{ provide: ConfigService, useValue: stub }` and calls `configService.get('API_URL')` above the provider, `configService.get(Env.BASE)` with `Env` imported from another file, and `configService.get(key)` with `key` a parameter
- **THEN** the file calls no `env` key, and has no unread site at those lines

#### Scenario: A read of an overridden ConfigService is no environment read
- **WHEN** a spec file calls `Test.createTestingModule({ imports: [AppModule] }).overrideProvider(ConfigService).useValue(stub)` and `configService.get('API_URL')`
- **THEN** the file calls no `env` key

#### Scenario: A test on the real ConfigService keeps its reads
- **WHEN** a spec file imports `ConfigModule.forRoot()`, lists `ConfigService` in `providers`, has `{ provide: BillingService, useValue: stub }` and calls `configService.get('API_URL')`
- **THEN** the file calls `env API_URL`

#### Scenario: A file that is no test keeps its reads though it provides a stand-in
- **WHEN** `src/app.module.ts` has `{ provide: ConfigService, useValue: stub }` and calls `configService.get('API_URL')`
- **THEN** the file calls `env API_URL`

#### Scenario: A project pack names the token its test replaces
- **WHEN** a project pack has a call rule on `settings.read` with `test_double = "Settings"`, one spec file has `{ provide: Settings, useClass: FakeSettings }` and calls `settings.read('PORT')`, and another spec file calls the same with no stand-in
- **THEN** only the second file calls the key
