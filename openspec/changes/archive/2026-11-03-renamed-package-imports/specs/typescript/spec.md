# Spec Delta

## ADDED Requirements

### Requirement: Imports under another name
A call rule SHALL read a name imported from a package a pack detects under the name the package gives it: the package for a default or namespace import or a `require`, the exported name for `{ a as b }`. A Node module, a path inside a package, a package whose name is no identifier, a name from a file of the project and a name a scope binds again SHALL keep their name.

#### Scenario: A renamed default import of a package is read
- **WHEN** a file has `import http from 'axios'`, `http.get('/a')`, `const api = http.create({ baseURL: '/v1' })` and `api.get('/d')`
- **THEN** it calls `http GET /a` and `http GET /v1/d`

#### Scenario: A namespace import of a package is read
- **WHEN** a file has `import * as ax from 'axios'` and `ax.post('/b')`
- **THEN** it calls `http POST /b`

#### Scenario: A renamed require of a package is read
- **WHEN** a file has `const axiosLib = require('axios')` and `axiosLib.put('/c')`
- **THEN** it calls `http PUT /c`

#### Scenario: A renamed destructured require of a package is read
- **WHEN** a file has `const { Router: R } = require('express')`, `const router = R()` and `router.get('/f', h)`
- **THEN** it serves `http GET /f`

#### Scenario: Renamed imports of a router are read
- **WHEN** a file has `import ex, { Router as R } from 'express'`, `const app = ex()`, `app.get('/e', h)`, `const router = R()` and `router.get('/f', h)`
- **THEN** it serves the same routes as a file that spells the names `express` and `Router`

#### Scenario: A name no pack detects keeps its own name
- **WHEN** a file has `import fetch from 'node-fetch'`, `fetch('/g')`, `import http from 'http'` and `http.get('/h')`
- **THEN** it calls `http GET /g` and nothing else

#### Scenario: A renamed import a scope binds again is not read
- **WHEN** a file has `import http from 'axios'` and `function f(http) { http.get('/i'); }`
- **THEN** it calls nothing
