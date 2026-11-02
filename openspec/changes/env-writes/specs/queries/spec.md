## ADDED Requirements

### Requirement: A file that sets and reads a contract
A file that both serves and calls a contract SHALL leave it as a server, so that a change to it reaches the files that call the contract and a change to a file that only calls it does not reach the file that serves it.

#### Scenario: A file that sets a variable reaches the files that read it
- **WHEN** `a.py` serves and calls `env X`, `b.py` calls `env X`, and `a.py` changes
- **THEN** impact lists `b.py` with the reason `calls env X`, and a change to `b.py` does not list `a.py`
