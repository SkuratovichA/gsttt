// The methods that a resolver calls, with the files that send a GraphQL operation the resolver serves: the candidates of the questions of the kind `clients` of `cargo xtask agents questions`.
//
//   node bench/agents/clients.mjs --tools <bench/tools/node> --checkout <dir> --scope <dir> --tsconfig <file>
//
// The answers come from the TypeScript language service (`service.mjs`, the same program as `functions.mjs` reads) and from the `graphql` package, not from gsttt, so that no tool under test marks its own work. Both are the ones installed in `--tools`.
// A resolver is a method of a class decorated with `@Resolver` that carries `@Query`, `@Mutation` or `@Subscription`, the three decorators of `@nestjs/graphql` by the name the file imports them under. It serves the field the decorator names with its `name` option or its string argument, else the field of the method's name, of the root type of the decorator.
// A candidate is a method with a body of a top-level class of the scope that is not a resolver, in a file that is not a test or a declaration file. It counts a resolver as calling it when a reference to it (`findReferences`) lies in the text of the resolver's method. Its clients are the tracked files of the scope that hold a GraphQL document (a `.graphql` or `.gql` file, or a `gql` or `graphql` tagged template or call with a template or a string as its first argument, in a TypeScript or JavaScript file) with an operation of the root type that selects that field at its top level: an alias selects the field it aliases, and a fragment spread or an inline fragment at the top level selects what it selects.
// The program is the TypeScript below the directory of the tsconfig, since the clients live in packages the tsconfig of the resolvers does not describe; the documents are looked for in the whole scope. A document that does not parse, a fragment spread of a fragment no document defines, a decorator argument that could name a field and that the script cannot read (not a function, a string or an object literal without a spread, or a name that is not a string), stops the script: an answer built without them would be wrong.
// Prints one JSON object on standard output, sorted so that the same checkout gives the same bytes:
//   { typescript, graphql, files, methods: [{ file, name, line, clients, ambiguous }] }
// With `--fields` it prints instead the answer at the level of the field: { typescript, graphql, files, fields: [{ field, servers: [{ file, line, method }], clients }] }, one entry for each `Root.field` that a resolver of the program serves or a document of the scope selects, sorted by `field`, the servers by file then line and the clients by path.
// `name` is `Class.method`; only methods that some resolver calls are listed, and `clients` may still be empty when no document selects the field. `ambiguous` is true when a field that such a resolver serves is also served under another root type by some resolver (`Query foo` and `Mutation foo`), so that the name of the field alone does not tell which operations are meant.

import fs from 'node:fs'
import path from 'node:path'

import { compare, isTestOrDeclaration, loadTool, openService, readOptions } from './service.mjs'

const options = readOptions({ fields: true })
const { ts, tools, tracked, files, absolute, relative, service, program } = openService(options, { program: path.posix.dirname(options.tsconfig) })
const graphql = loadTool(tools, 'graphql')
const checker = program.getTypeChecker()

const ROOTS = ['Query', 'Mutation', 'Subscription']

/** What `source` imports of `@nestjs/graphql`: the names it exports by the name the file gives them, and the namespaces the whole module is bound to (`import * as graphql from ...`). */
function nestImports(source) {
  const names = new Map()
  const spaces = new Set()
  for (const statement of source.statements) {
    if (!ts.isImportDeclaration(statement) || !ts.isStringLiteral(statement.moduleSpecifier) || statement.moduleSpecifier.text !== '@nestjs/graphql') continue
    const bindings = statement.importClause?.namedBindings
    if (!bindings) continue
    if (ts.isNamespaceImport(bindings)) {
      spaces.add(bindings.name.text)
      continue
    }
    for (const element of bindings.elements) names.set(element.name.text, (element.propertyName ?? element.name).text)
  }
  return { names, spaces }
}

/** The decorators of `node` that come from `@nestjs/graphql`, each with the name it has there and its arguments. */
function nestDecorators(node, imports) {
  const found = []
  for (const decorator of ts.canHaveDecorators(node) ? (ts.getDecorators(node) ?? []) : []) {
    const call = ts.isCallExpression(decorator.expression) ? decorator.expression : undefined
    const callee = call ? call.expression : decorator.expression
    let name
    if (ts.isIdentifier(callee)) name = imports.names.get(callee.text)
    else if (ts.isPropertyAccessExpression(callee) && ts.isIdentifier(callee.expression) && imports.spaces.has(callee.expression.text)) name = callee.name.text
    if (name) found.push({ name, args: call ? [...call.arguments] : [] })
  }
  return found
}

/** Where `node` is, for a message. */
function where(node) {
  const source = node.getSourceFile()
  return `${relative(source.fileName)}:${source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1}`
}

/** The string an expression is: a literal, or anything the type checker knows to be one string, such as a `const`. */
function stringOf(expression) {
  if (ts.isStringLiteralLike(expression)) return expression.text
  const type = checker.getTypeAtLocation(expression)
  if (type.isStringLiteral()) return type.value
  throw new Error(`${where(expression)}: the name in the decorator is not a string the script can read`)
}

/** The field a resolver method serves: the `name` of an options object or a string as the first argument, else the name of the method. An argument that could hide the name from the script stops it, since the method name would then be the wrong field. */
function fieldOf(decorator, method) {
  let field
  for (const [index, arg] of decorator.args.entries()) {
    if (ts.isArrowFunction(arg) || ts.isFunctionExpression(arg)) continue
    if (index === 0 && ts.isStringLiteralLike(arg)) {
      field ??= arg.text
    } else if (ts.isObjectLiteralExpression(arg)) {
      for (const property of arg.properties) {
        if (ts.isSpreadAssignment(property)) throw new Error(`${where(property)}: the options of @${decorator.name} have a spread, which the script cannot read the name from`)
        if (ts.isShorthandPropertyAssignment(property) && property.name.text === 'name') field ??= stringOf(property.name)
        if (ts.isPropertyAssignment(property) && (ts.isIdentifier(property.name) || ts.isStringLiteral(property.name)) && property.name.text === 'name') field ??= stringOf(property.initializer)
      }
    } else {
      throw new Error(`${where(arg)}: the argument \`${arg.getText()}\` of @${decorator.name} is neither a function, a string as the first argument nor an object literal, so the script cannot tell the field it serves`)
    }
  }
  if (field !== undefined) return field
  if (!ts.isIdentifier(method.name)) throw new Error(`${where(method)}: a resolver whose name is not an identifier and is not given`)
  return method.name.text
}

// The resolvers by file: the text each covers, the root type and the field it serves. The candidates: the methods of every other class.
const resolvers = new Map()
const candidates = []
for (const file of files) {
  if (isTestOrDeclaration(file)) continue
  const source = program.getSourceFile(absolute(file))
  const imports = nestImports(source)
  for (const statement of source.statements) {
    if (!ts.isClassDeclaration(statement) || !statement.name) continue
    if (nestDecorators(statement, imports).some((decorator) => decorator.name === 'Resolver')) {
      for (const method of statement.members.filter(ts.isMethodDeclaration)) {
        for (const decorator of nestDecorators(method, imports)) {
          if (!ROOTS.includes(decorator.name)) continue
          const served = { root: decorator.name, field: fieldOf(decorator, method), start: method.getStart(source), end: method.end, file, method: `${statement.name.text}.${method.name.getText(source)}`, line: source.getLineAndCharacterOfPosition(method.name.getStart(source)).line + 1 }
          resolvers.set(file, [...(resolvers.get(file) ?? []), served])
        }
      }
    } else {
      for (const method of statement.members.filter(ts.isMethodDeclaration)) {
        if (method.body && ts.isIdentifier(method.name)) candidates.push({ file, source, name: `${statement.name.text}.${method.name.text}`, identifier: method.name })
      }
    }
  }
}

/** The name of each field that the selection set `set` of a document of `file` selects at its top level, spreading the fragments it names. */
function selectedAt(set, file, fragments) {
  const fields = []
  for (const selection of set.selections) {
    if (selection.kind === graphql.Kind.FIELD) {
      fields.push(selection.name.value)
    } else if (selection.kind === graphql.Kind.INLINE_FRAGMENT) {
      fields.push(...selectedAt(selection.selectionSet, file, fragments))
    } else {
      const name = selection.name.value
      const defined = fragments.get(name) ?? []
      const own = defined.filter((fragment) => fragment.file === file)
      const found = own.length > 0 ? own : defined
      if (found.length !== 1) throw new Error(`${file}: the fragment \`${name}\` is ${found.length === 0 ? 'defined in no document' : 'defined in several documents'}`)
      fields.push(...selectedAt(found[0].node.selectionSet, found[0].file, fragments))
    }
  }
  return fields
}

/** Whether an identifier names the function or tag that marks a GraphQL document. */
function isDocumentTag(identifier) {
  return identifier.text === 'gql' || identifier.text === 'graphql'
}

/** The GraphQL documents a file holds, as `{ text, line }` with the line the text starts on: the whole file for `.graphql` and `.gql`, each `gql` or `graphql` tagged template, or call with a template or a string as its first argument, for a script. An expression in a template is dropped, which keeps the fragments it names for the documents that define them. */
function documentsOf(file) {
  // The name decides before the read: a tracked symlink to nothing or a submodule path is not a document and is never opened.
  const isDocument = /\.(graphql|gql)$/.test(file)
  if (!isDocument && !/\.(ts|tsx|mts|cts|js|jsx|mjs|cjs)$/.test(file)) return []
  const text = fs.readFileSync(absolute(file), 'utf8')
  if (isDocument) return [{ text, line: 1 }]
  if (!/\b(gql|graphql)\s*(`|\(\s*[`'"])/.test(text)) return []
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true)
  const found = []
  const visit = (node) => {
    const tagged = ts.isTaggedTemplateExpression(node) && ts.isIdentifier(node.tag) && isDocumentTag(node.tag) ? node.template : undefined
    // The `graphql(\`…\`)` call of the client preset of graphql-codegen.
    const called = ts.isCallExpression(node) && ts.isIdentifier(node.expression) && isDocumentTag(node.expression) ? node.arguments[0] : undefined
    const template = tagged ?? called
    if (template && ts.isNoSubstitutionTemplateLiteral(template)) {
      found.push({ text: template.text, line: source.getLineAndCharacterOfPosition(template.getStart(source)).line + 1 })
    } else if (template && ts.isTemplateExpression(template)) {
      found.push({ text: template.head.text + template.templateSpans.map((span) => ` ${span.literal.text}`).join(''), line: source.getLineAndCharacterOfPosition(template.getStart(source)).line + 1 })
    } else if (called && ts.isStringLiteral(called)) {
      found.push({ text: called.text, line: source.getLineAndCharacterOfPosition(called.getStart(source)).line + 1 })
    }
    ts.forEachChild(node, visit)
  }
  visit(source)
  return found
}

// Every document of the scope, parsed; then the files that select each field of each root type.
const operations = []
const fragments = new Map()
for (const file of [...tracked].sort(compare)) {
  for (const { text, line } of documentsOf(file)) {
    let document
    try {
      document = graphql.parse(text, { noLocation: true })
    } catch (error) {
      throw new Error(`${file}:${line}: the document does not parse: ${error.message}`)
    }
    for (const definition of document.definitions) {
      if (definition.kind === graphql.Kind.OPERATION_DEFINITION) {
        operations.push({ file, root: definition.operation[0].toUpperCase() + definition.operation.slice(1), node: definition })
      } else if (definition.kind === graphql.Kind.FRAGMENT_DEFINITION) {
        fragments.set(definition.name.value, [...(fragments.get(definition.name.value) ?? []), { file, node: definition }])
      }
    }
  }
}
const clientsOf = new Map()
for (const { file, root, node } of operations) {
  for (const field of selectedAt(node.selectionSet, file, fragments)) {
    const key = `${root} ${field}`
    clientsOf.set(key, (clientsOf.get(key) ?? new Set()).add(file))
  }
}

// The root types that serve each field name over all resolvers: two make a method that calls either of them ambiguous.
const rootsOf = new Map()
for (const served of [...resolvers.values()].flat()) rootsOf.set(served.field, (rootsOf.get(served.field) ?? new Set()).add(served.root))

const resolverText = [...resolvers.keys()].map((file) => program.getSourceFile(absolute(file)).text).join('\n')
if (options.fields) {
  const servers = new Map()
  for (const served of [...resolvers.values()].flat()) {
    const key = `${served.root}.${served.field}`
    servers.set(key, [...(servers.get(key) ?? []), { file: served.file, line: served.line, method: served.method }])
  }
  const clients = new Map([...clientsOf].map(([key, set]) => [key.replace(' ', '.'), [...set].sort(compare)]))
  const fields = [...new Set([...servers.keys(), ...clients.keys()])].sort(compare).map((field) => ({
    field,
    servers: (servers.get(field) ?? []).sort((a, b) => compare(a.file, b.file) || a.line - b.line),
    clients: clients.get(field) ?? [],
  }))
  process.stdout.write(JSON.stringify({ typescript: ts.version, graphql: graphql.version, files: files.length, fields }))
} else {
  const methods = []
  for (const { file, source, name, identifier } of candidates) {
    // A reference to a method has its name in the text, so a name no resolver file holds needs no search.
    if (!resolverText.includes(identifier.text)) continue
    const served = new Map()
    for (const symbol of service.findReferences(absolute(file), identifier.getStart(source)) ?? []) {
      for (const reference of symbol.references) {
        for (const resolver of resolvers.get(relative(reference.fileName)) ?? []) {
          if (resolver.start <= reference.textSpan.start && reference.textSpan.start < resolver.end) served.set(`${resolver.root} ${resolver.field}`, resolver)
        }
      }
    }
    if (served.size === 0) continue
    const clients = new Set([...served.keys()].flatMap((key) => [...(clientsOf.get(key) ?? [])]))
    const ambiguous = [...served.values()].some((resolver) => rootsOf.get(resolver.field).size > 1)
    methods.push({ file, name, line: source.getLineAndCharacterOfPosition(identifier.getStart(source)).line + 1, clients: [...clients].sort(compare), ambiguous })
  }
  methods.sort((a, b) => compare(a.file, b.file) || compare(a.name, b.name) || a.line - b.line)

  process.stdout.write(JSON.stringify({ typescript: ts.version, graphql: graphql.version, files: files.length, methods }))
}
