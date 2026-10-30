// The methods that a resolver calls, with the files that send a GraphQL operation the resolver serves: the candidates of the questions of the kind `clients` of `cargo xtask agents questions`.
//
//   node bench/agents/clients.mjs --tools <bench/tools/node> --checkout <dir> --scope <dir> --tsconfig <file>
//
// The answers come from the TypeScript language service (`service.mjs`, the same program as `functions.mjs` reads) and from the `graphql` package, not from gsttt, so that no tool under test marks its own work. Both are the ones installed in `--tools`.
// A resolver is a method of a class decorated with `@Resolver` that carries `@Query`, `@Mutation` or `@Subscription`, the three decorators of `@nestjs/graphql` by the name the file imports them under. It serves the field the decorator names with its `name` option or its string argument, else the field of the method's name, of the root type of the decorator.
// A candidate is a method with a body of a top-level class of the scope that is not a resolver, in a file that is not a test or a declaration file. It counts a resolver as calling it when a reference to it (`findReferences`) lies in the text of the resolver's method. Its clients are the tracked files of the scope that hold a GraphQL document (a `.graphql` or `.gql` file, or a `gql` or `graphql` tagged template of a TypeScript or JavaScript file) with an operation of the root type that selects that field at its top level: an alias selects the field it aliases, and a fragment spread or an inline fragment at the top level selects what it selects.
// The program is the TypeScript below the directory of the tsconfig, since the clients live in packages the tsconfig of the resolvers does not describe; the documents are looked for in the whole scope. A document that does not parse, a fragment spread of a fragment no document defines, or a name a decorator gives that is not a string, stops the script: an answer built without them would be wrong.
// Prints one JSON object on standard output, sorted so that the same checkout gives the same bytes:
//   { typescript, graphql, files, methods: [{ file, name, line, clients }] }
// `name` is `Class.method`; only methods that some resolver calls are listed, and `clients` may still be empty when no document selects the field.

import fs from 'node:fs'
import path from 'node:path'

import { compare, isTestOrDeclaration, loadTool, openService, readOptions } from './service.mjs'

const options = readOptions()
const { ts, tools, tracked, files, absolute, relative, service, program } = openService(options, { program: path.posix.dirname(options.tsconfig) })
const graphql = loadTool(tools, 'graphql')
const checker = program.getTypeChecker()

const ROOTS = ['Query', 'Mutation', 'Subscription']

/** The names `@nestjs/graphql` exports that `source` imports, by the name the file gives them. */
function nestImports(source) {
  const names = new Map()
  for (const statement of source.statements) {
    if (!ts.isImportDeclaration(statement) || !ts.isStringLiteral(statement.moduleSpecifier) || statement.moduleSpecifier.text !== '@nestjs/graphql') continue
    const bindings = statement.importClause?.namedBindings
    if (!bindings || !ts.isNamedImports(bindings)) continue
    for (const element of bindings.elements) names.set(element.name.text, (element.propertyName ?? element.name).text)
  }
  return names
}

/** The decorators of `node` that come from `@nestjs/graphql`, each with the name it has there and its arguments. */
function nestDecorators(node, imports) {
  const found = []
  for (const decorator of ts.canHaveDecorators(node) ? (ts.getDecorators(node) ?? []) : []) {
    const call = ts.isCallExpression(decorator.expression) ? decorator.expression : undefined
    const callee = call ? call.expression : decorator.expression
    const name = ts.isIdentifier(callee) ? imports.get(callee.text) : undefined
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

/** The field a resolver method serves: the `name` of an options object or a string as the first argument, else the name of the method. */
function fieldOf(decorator, method) {
  for (const [index, arg] of decorator.args.entries()) {
    if (ts.isObjectLiteralExpression(arg)) {
      for (const property of arg.properties) {
        if (ts.isShorthandPropertyAssignment(property) && property.name.text === 'name') return stringOf(property.name)
        if (ts.isPropertyAssignment(property) && (ts.isIdentifier(property.name) || ts.isStringLiteral(property.name)) && property.name.text === 'name') return stringOf(property.initializer)
      }
    } else if (index === 0 && ts.isStringLiteralLike(arg)) {
      return arg.text
    }
  }
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
          const served = { root: decorator.name, field: fieldOf(decorator, method), start: method.getStart(source), end: method.end }
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

/** The GraphQL documents a file holds, as `{ text, line }` with the line the text starts on: the whole file for `.graphql` and `.gql`, each `gql` or `graphql` tagged template for a script. An expression in a template is dropped, which keeps the fragments it names for the documents that define them. */
function documentsOf(file) {
  const text = fs.readFileSync(absolute(file), 'utf8')
  if (/\.(graphql|gql)$/.test(file)) return [{ text, line: 1 }]
  if (!/\.(ts|tsx|mts|cts|js|jsx|mjs|cjs)$/.test(file) || !/\b(gql|graphql)\s*`/.test(text)) return []
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true)
  const found = []
  const visit = (node) => {
    if (ts.isTaggedTemplateExpression(node) && ts.isIdentifier(node.tag) && (node.tag.text === 'gql' || node.tag.text === 'graphql')) {
      const template = node.template
      const body = ts.isNoSubstitutionTemplateLiteral(template) ? template.text : template.head.text + template.templateSpans.map((span) => ` ${span.literal.text}`).join('')
      found.push({ text: body, line: source.getLineAndCharacterOfPosition(template.getStart(source)).line + 1 })
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

const resolverText = [...resolvers.keys()].map((file) => program.getSourceFile(absolute(file)).text).join('\n')
const methods = []
for (const { file, source, name, identifier } of candidates) {
  // A reference to a method has its name in the text, so a name no resolver file holds needs no search.
  if (!resolverText.includes(identifier.text)) continue
  const served = new Set()
  for (const symbol of service.findReferences(absolute(file), identifier.getStart(source)) ?? []) {
    for (const reference of symbol.references) {
      for (const resolver of resolvers.get(relative(reference.fileName)) ?? []) {
        if (resolver.start <= reference.textSpan.start && reference.textSpan.start < resolver.end) served.add(`${resolver.root} ${resolver.field}`)
      }
    }
  }
  if (served.size === 0) continue
  const clients = new Set([...served].flatMap((key) => [...(clientsOf.get(key) ?? [])]))
  methods.push({ file, name, line: source.getLineAndCharacterOfPosition(identifier.getStart(source)).line + 1, clients: [...clients].sort(compare) })
}
methods.sort((a, b) => compare(a.file, b.file) || compare(a.name, b.name) || a.line - b.line)

process.stdout.write(JSON.stringify({ typescript: ts.version, graphql: graphql.version, files: files.length, methods }))
