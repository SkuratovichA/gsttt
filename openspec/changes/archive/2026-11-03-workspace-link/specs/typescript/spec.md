# Spec Delta

## ADDED Requirements

### Requirement: Constants of other packages in the link
The link of a TypeScript project SHALL read a name declared in a file of another package that its files import, in a URL and in the path of a mount, as it reads a name of its own project. A name that cannot be read SHALL leave the URL or the mount as unread as before.

#### Scenario: A URL reads a constant of another package
- **WHEN** a file imports `API_PREFIX` and `USERS` from another package and calls `axios.get` with a template and with a concatenation of them
- **THEN** it calls the paths the constants spell

#### Scenario: A mount reads a constant of another package
- **WHEN** an app mounts a router with `app.use(API_PREFIX, users)` and `API_PREFIX` is declared in another package
- **THEN** the router serves its routes under the constant

#### Scenario: A prefix that reads through another package leaves no unread site
- **WHEN** an app mounts a router at a template made of a constant of another package of the repository and a literal
- **THEN** the router serves under the path the template reads as, and the app has no unread site

#### Scenario: A prefix whose name comes from an outside package stays an unread site
- **WHEN** an app mounts a router at a template made of a name that a package outside the repository declares
- **THEN** the mount is one unread site of the app

### Requirement: Routers of other packages in the link
An application SHALL serve the routes of a router of another package under the path it mounts the router at, below the router's own mounts, and a router that no file mounts SHALL keep the path it is written with. The routes of a router that several applications mount SHALL be served under each mount.

#### Scenario: A router of another package mounted by an app serves under the mount
- **WHEN** an app mounts a router of another package at `/ext`, and the router has `get('/x')`
- **THEN** the router serves `http GET /ext/x` and not `http GET /x`

#### Scenario: A router of another package with a router of its own below it chains the prefixes
- **WHEN** an app mounts a router of another package at `/ext`, and that router mounts a second router at `/inner` which has `get('/x')`
- **THEN** the second router serves `http GET /ext/inner/x`

#### Scenario: Two apps mounting one router give both paths
- **WHEN** two apps mount one router of another package at `/api` and at `/admin`
- **THEN** the router serves its route under both mounts

#### Scenario: Routers imported by name under a mount each serve under it
- **WHEN** a router passes two imported routers `ra` and `rb` to its `use`, each with one route, and an app mounts that router at `/w`
- **THEN** the routes are served as `http GET /w/ra` and `http GET /w/rb`

#### Scenario: A router of another package that no app mounts keeps its path
- **WHEN** a router of another package is imported and no file mounts it
- **THEN** it serves the path it is written with

### Requirement: Controllers of other packages in the link
A NestJS application SHALL serve a controller of another package behind its global prefix when the modules it imports reach the controller. A controller no application reaches SHALL keep the path it is written with. A package that is not in the repository SHALL give no site and no unread site.

#### Scenario: A controller of another package serves behind the global prefix of the app that reaches it
- **WHEN** an app sets the global prefix `api` and imports a module of another package that holds a controller at `items`, and another controller of that package is in no module
- **THEN** the first serves `http GET /api/items/{}` and the second serves `http GET /legacy/{}`

#### Scenario: An outside package imported by name adds no site and no blind spot
- **WHEN** a file imports a router from a package that is not in the repository and mounts it
- **THEN** the file gets no new site and no new unread site
