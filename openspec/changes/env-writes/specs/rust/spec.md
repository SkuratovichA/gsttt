## ADDED Requirements

### Requirement: Environment writes
With the `env` pack, `env::set_var`, `std::env::set_var`, `env::remove_var` and `std::env::remove_var` with a literal name SHALL serve `env <NAME>`, inside an `unsafe` block as well. A chain rule with a `written` role SHALL name a field that is assigned, with `=` or a compound operator, under that role, and a field that is read under its `role`.

#### Scenario: Setting or removing a variable serves it
- **WHEN** a file has `env::set_var("A", "1")`, `std::env::set_var("B", "2")`, `env::remove_var("C")`, `std::env::remove_var("D")` and `env::set_var("E", "3")` inside `unsafe { }`
- **THEN** it serves `env A`, `env B`, `env C`, `env D` and `env E` and calls no `env`

#### Scenario: A chain rule with a written role serves what is assigned
- **WHEN** a chain rule has the root `settings`, `role = "calls"` and `written = "serves"`, and a file has `settings.port = 8080;`, `settings.retries += 1;` and `let p = settings.port;`
- **THEN** it serves `env PORT` and `env RETRIES` and calls `env PORT`
