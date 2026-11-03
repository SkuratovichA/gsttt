# Spec Delta

## ADDED Requirements

### Requirement: Packages required in place
A call rule SHALL read a callee that starts at `require('pkg')` with a literal argument under the name a pack writes for the package, and a local bound to a member of such a require under that name. A package no pack reads, a path of the project, a Node module, a path inside a package and a computed member SHALL give no fact.

#### Scenario: A member of a package required in place is called
- **WHEN** a function has `return require('axios').get('/c')`
- **THEN** it calls `http GET /c`

#### Scenario: A router required in place serves its routes
- **WHEN** a file has `const router = require('express').Router()` and `router.get('/a', h)`
- **THEN** it serves `http GET /a`

#### Scenario: An application required in place serves its routes
- **WHEN** a file has `const app = require('express')()` and `app.get('/b', h)`
- **THEN** it serves `http GET /b`

#### Scenario: A member of a package bound to a name is called
- **WHEN** a file has `const get = require('axios').get` and `get('/d')`
- **THEN** it calls `http GET /d`

#### Scenario: A member of a project module bound to a name is read as the export
- **WHEN** a file has `const client = require('./named').client`, `client.get('/users')`, and `./named` exports a client made by `axios.create({ baseURL: '/named' })`
- **THEN** it calls `http GET /named/users`

#### Scenario: A require in place that no pack reads gives no fact
- **WHEN** a file has `require('left-pad').get('/lp')` and `require('./x').get('/y')`
- **THEN** it has no contract

#### Scenario: A computed member of a required package gives no fact
- **WHEN** a file has `require('axios')[name]('/x')`
- **THEN** it has no contract
