# Spec Delta

## ADDED Requirements

### Requirement: A caller is told apart by the member it names
Asked about a member, `impact` SHALL NOT list a declaration whose only uses of the class name other members, whether it reaches them through a new instance, a local value, a parameter or the class itself.

#### Scenario: A caller of another member through a new instance is not listed
- **WHEN** `service.ts` declares `OrderService` with `place` and `cancel`, `refunds.ts` has `new OrderService().cancel(id)` in `refund`, `checkout.ts` has `new OrderService().place(id)` in `pay`, and impact is asked about `OrderService.place` in `service.ts`
- **THEN** impact lists `checkout.ts` with the caller `pay`
- **AND** it does not list `refunds.ts`

#### Scenario: Asked about the class, every caller of a member is listed
- **WHEN** the files are those of the previous scenario, with also `keep.ts` declaring `function keep(service: OrderService) {}`, and impact is asked about `OrderService`
- **THEN** impact lists `checkout.ts`, `refunds.ts` and `keep.ts`
