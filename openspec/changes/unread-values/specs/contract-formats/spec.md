# Spec Delta

## ADDED Requirements

### Requirement: A literal alternative
A literal alternative of a rule's value, as in `$arg2.method|GET`, SHALL stand in only for a value the site does not carry. An argument, a property or a keyword that the site holds and no connector can read as text SHALL leave the value unread, and a method that is unread SHALL make the contract any method (`*`).

#### Scenario: A literal stands in only for a value the site does not carry
- **WHEN** a rule has the key `$arg1` and the method `$arg2.method|GET`, and a site has `/x` as its first argument
- **THEN** a second argument with no `method` reads `http GET /x`
- **AND** a second argument whose `method` cannot be read as text reads `http * /x`
- **AND** a `method` of `PUT` reads `http PUT /x`

### Requirement: A decorator prefix that is not text
A decorator rule whose `prefix` the site holds and no connector can read as text SHALL make the decorator a blind spot and declare no fact, since joining nothing would serve the contract at a path that is not its own. A `prefix` the site does not carry SHALL add nothing to the path.

#### Scenario: A decorator whose own prefix is not text is unread
- **WHEN** a decorator rule has the key `$arg1` and the prefix `$arg2.prefix`, and the decorator is read behind the outer prefix `/api` with `/x` as its first argument
- **THEN** a `prefix` that cannot be read as text makes it a blind spot and declares no fact
- **AND** no `prefix` declares `http GET /api/x`, and a `prefix` of `v1` declares `http GET /api/v1/x`
