# Spec Delta

## ADDED Requirements

### Requirement: Fields of a declared type
Inside a class, a declaration that reads `this.<field>` SHALL also use the names the field's type stands for at its head, read as for root types, when the file declares or binds them. A field is a constructor parameter with `private`, `protected`, `public` or `readonly`, or a property with a type annotation or a `new` initializer, so a method that calls an injected service uses the class of the service.

#### Scenario: A method that calls an injected service uses its class
- **WHEN** `src/invoices.resolver.ts` imports `InvoiceService`, `MailQueue` and `JobQueue` from `./services` and declares `class InvoiceResolver { private readonly mail: MailQueue; private jobs = new JobQueue(); constructor(private readonly invoices: InvoiceService, plain: InvoiceService) {} issue() { this.invoices.issue(); this.mail.push(); this.jobs.add() } other() { return this.plain } }`
- **THEN** `InvoiceResolver.issue` uses `InvoiceService`, `MailQueue` and `JobQueue`
- **AND** `InvoiceResolver.other` uses none of them, because a constructor parameter without a modifier is no field

#### Scenario: A field is read in its own class
- **WHEN** `src/jobs.ts` imports `InvoiceService` and `MailQueue` from `./services` and declares `class Billing { constructor(private readonly svc: InvoiceService) {} }` and `class Mailer { constructor(private readonly svc: MailQueue) {} send() { this.svc.push() } }`
- **THEN** `Mailer.send` uses `MailQueue` and not `InvoiceService`
