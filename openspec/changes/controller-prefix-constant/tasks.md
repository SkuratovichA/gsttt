# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The parse

- [x] 2.1 `ClassPrefix` moves to `shape.rs` with `ClassPrefix::read` and the flag that says the prefix waits for the link
- [x] 2.2 `WaitingPrefix` and `DeferredTarget::ClassUnder` in `extra.rs`, and a waiting prefix in place of an unread site for a named class that serves routes
- [x] 2.3 The method decorators of a class with a waiting prefix are deferred, and the inherit of such a class is left to the link

## 3. The link

- [x] 3.1 `deferred.rs` reads the waiting prefixes, reports an unreadable one once, and keeps the inherited routes of a controller for each text
- [x] 3.2 A deferred decorator is applied under each text of its class prefix and placed as a class route
- [x] 3.3 `provides.rs`: the blind spot about a controller prefix is narrowed, and `READER_VERSION` goes from 13 to 14

## 4. The scenarios

- [x] 4.1 Fixtures and tests for the nine scenarios of "Controller prefixes from constants of other files"
- [x] 4.2 The fixture of "A controller whose prefix is not read serves nothing" holds a prefix that is a call in the other file
