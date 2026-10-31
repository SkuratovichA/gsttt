# Spec Delta

## MODIFIED Requirements

### Requirement: A computed origin alone
A call whose request target is only a computed origin SHALL be a blind spot and SHALL declare no fact about the path `/`. A call whose request target is computed and whose path keeps no literal segment, every segment being a parameter, SHALL be a blind spot too, since a path of parameters alone meets every served path of its length.

#### Scenario: A computed origin alone is a blind spot
- **WHEN** a call has the request target `${}`, `https://${}`, `${}://${}` or `${}//${}`, or one that cannot be read
- **THEN** it is a blind spot and declares no fact

#### Scenario: A computed target with a literal segment is a fact
- **WHEN** a call has the request target `${}/users/${}`
- **THEN** it declares `http <method> /users/{}`

#### Scenario: A target whose path is only parameters is a blind spot
- **WHEN** a call has the request target `${}/${}`, `${}${}/${}` or `https://${}/${}/${}`
- **THEN** it is a blind spot and declares no fact
