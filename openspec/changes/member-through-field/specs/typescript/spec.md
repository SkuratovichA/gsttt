## REMOVED Requirements

### Requirement: Fields of a declared type
**Reason**: A call through a field uses the member it calls, not the whole class of the field, so the scenarios that listed the class change their meaning. They move to `Members read through a field of a declared type`, which also keeps a read of the field as a value using the class.
**Migration**: A read of `this.<field>` as a value uses the class as before. `this.<field>.<member>` uses `Class.member`, spelled as the type of the field writes the class.

## ADDED Requirements

### Requirement: Members read through a field of a declared type
Inside a class, a declaration that reads `this.<field>.<member>`, or `this.#<field>.<member>`, SHALL use `<Type>.<member>` for each name `<Type>` the field's type stands for at its head, spelled as the type writes it, when the file declares or binds `<Type>`. Any other read of the field SHALL use `<Type>` alone. A field is a constructor parameter with `private`, `protected`, `public` or `readonly`, or a property with a type annotation or a `new` initializer.

#### Scenario: A method that calls an injected service uses the member it calls
- **WHEN** `src/invoices.resolver.ts` imports `InvoiceService`, `MailQueue` and `JobQueue` from `./services` and declares `class InvoiceResolver { private readonly mail: MailQueue; private jobs = new JobQueue(); constructor(private readonly invoices: InvoiceService, plain: InvoiceService) {} issue() { this.invoices.issue(); this.mail.push(); this.jobs.add() } other() { return this.plain } }`
- **THEN** `InvoiceResolver.issue` uses `InvoiceService.issue`, `MailQueue.push` and `JobQueue.add`
- **AND** `InvoiceResolver.other` uses none of them, because a constructor parameter without a modifier is no field

#### Scenario: A field read as a value uses its class
- **WHEN** `src/invoices.resolver.ts` declares the class of the previous scenario with the method `all() { return this.invoices }`
- **THEN** `InvoiceResolver.all` uses `InvoiceService` and not a member of it

#### Scenario: A member of a field is read in its own class
- **WHEN** `src/jobs.ts` imports `InvoiceService` and `MailQueue` from `./services` and declares `class Billing { constructor(private readonly svc: InvoiceService) {} }` and `class Mailer { constructor(private readonly svc: MailQueue) {} send() { this.svc.push() } }`
- **THEN** `Mailer.send` uses `MailQueue.push` and neither `InvoiceService` nor a member of it
- **AND** `Billing` uses nothing, because no method of it reads the field

#### Scenario: A member of a private field is read through the field
- **WHEN** `src/mailer.ts` imports `JobQueue` and `MailQueue` from `./services` and declares `class Mailer { #queue: MailQueue; #jobs = new JobQueue(); queue = 0; send() { this.#queue.push() } later() { this.#jobs.add() } count() { return this.queue } }`
- **THEN** `Mailer.send` uses `MailQueue.push` and `Mailer.#queue`, and `Mailer.later` uses `JobQueue.add` and `Mailer.#jobs`
- **AND** `Mailer.count` does not use `MailQueue`, because `queue` and `#queue` are two fields and only the private one has a type

#### Scenario: A member is used as the type of the field is written
- **WHEN** `src/notifier.ts` imports `* as services` from `./services` and `JobQueue` and `MailQueue` from `./queues`, declares `class Local { run() {} stop() {} }`, and declares `class Notifier` with the fields `mail: services.MailQueue`, `either: MailQueue | JobQueue` and `local: Local`, whose methods `qualified`, `union`, `declared`, `undeclared`, `nested` and `indexed` read `this.mail.push()`, `this.either.send()`, `this.local.run()`, `this.local.start()`, `this.mail.items.push()` and `this.mail[key]`
- **THEN** `qualified` uses `services.MailQueue.push` and `union` uses `JobQueue.send` and `MailQueue.send`
- **AND** `declared` uses `Local.run` and `undeclared` uses `Local.start`, though `Local` declares no `start`
- **AND** `nested` uses `services.MailQueue.items`, because only the first member read from the field is the member, and `indexed` uses `services`, because a computed key names no member
