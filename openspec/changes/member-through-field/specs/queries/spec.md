## ADDED Requirements

### Requirement: Members called through a field
A use of `Type.member` SHALL refer to that member only, where a use of `Type` alone refers to every member. Asked about a member, `impact` SHALL list the declarations that use it or use its class alone, and not those that use another member. Asked about a class, it SHALL list those that use the class or any member of it. A member SHALL go on to the files that import its class as the class does, and a namespace import SHALL take every member.

#### Scenario: A call through a field names the member
- **WHEN** `service.ts` declares `InvoiceService` with the methods `issue` and `cancel`, `resolver.ts` imports it and declares `InvoiceResolver.issueInvoice`, which uses `InvoiceService.issue`, `InvoiceResolver.cancelInvoice`, which uses `InvoiceService.cancel`, and `InvoiceResolver.all`, which uses `InvoiceService`, and impact is asked about `InvoiceService.issue` in `service.ts`
- **THEN** impact lists `resolver.ts` with the callers `InvoiceResolver.all` and `InvoiceResolver.issueInvoice`
- **AND** it does not list `InvoiceResolver.cancelInvoice`

#### Scenario: Asked about the class, a call of any member is listed
- **WHEN** the files are those of the previous scenario and impact is asked about `InvoiceService` in `service.ts`
- **THEN** impact lists `resolver.ts` with the callers `InvoiceResolver.all`, `InvoiceResolver.cancelInvoice` and `InvoiceResolver.issueInvoice`

#### Scenario: A member goes on to the files that import its class
- **WHEN** `page.ts` imports `InvoiceResolver` from `resolver.ts` and declares `Page.open`, which uses `InvoiceResolver.issueInvoice`, `Page.close`, which uses `InvoiceResolver.cancelInvoice`, and `Page.all`, which uses `InvoiceResolver`, and impact is asked about `InvoiceService.issue` in `service.ts` with depth 2
- **THEN** impact lists `page.ts` at distance 2 with the callers `Page.all` and `Page.open`, and not `Page.close`

#### Scenario: A member goes on under the name a barrel exports its class by
- **WHEN** `index.ts` re-exports `InvoiceResolver` as `Res` from `resolver.ts`, `page.ts` imports `Res` from `index.ts` and its `Page.open` uses `Res.issueInvoice`, `Page.close` uses `Res.cancelInvoice` and `Page.all` uses `Res`, and impact is asked about `InvoiceService.issue` in `service.ts` with depth 2
- **THEN** impact lists `index.ts` at distance 2, and `page.ts` at distance 2 with the callers `Page.all` and `Page.open`

#### Scenario: A namespace import takes every member of its class
- **WHEN** `page.ts` has `import * as res from './resolver'` and declares `Page.open`, which uses `res.InvoiceResolver.issueInvoice`, `Page.close`, which uses `res.InvoiceResolver.cancelInvoice`, and `Page.all`, which uses `res`, and impact is asked about `InvoiceService.issue` in `service.ts` with depth 2
- **THEN** impact lists `page.ts` at distance 2, imports `resolver.ts`, with the callers `Page.all`, `Page.close` and `Page.open`

#### Scenario: A member a subclass inherits reaches the callers of the subclass
- **WHEN** `audit.ts` imports `InvoiceService` from `service.ts` and declares `AuditedService`, which uses `InvoiceService`, `page.ts` imports `AuditedService` and declares `Page.open`, which uses `AuditedService.issue`, and `Page.idle`, which uses nothing, and impact is asked about `InvoiceService.issue` in `service.ts` with depth 2
- **THEN** impact lists `audit.ts` at distance 1 with the caller `AuditedService`, and `page.ts` at distance 2 with the caller `Page.open` and not `Page.idle`

