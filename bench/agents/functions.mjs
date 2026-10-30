// The exported functions of a scope of a TypeScript checkout, with the files that refer to each of them: the candidates of `cargo xtask agents questions`.
//
//   node bench/agents/functions.mjs --tools <bench/tools/node> --checkout <dir> --scope <dir> --tsconfig <file>
//
// The answers come from the TypeScript language service (`findReferences`), not from gsttt, so that no tool under test marks its own work. `service.mjs` builds it, and refuses to answer while a relative import or a path of the tsconfig does not resolve to a file.
// Prints one JSON object on standard output, sorted so that the same checkout gives the same bytes:
//   { typescript, files, functions: [{ file, name, line, bodyOffset, users }] }
// `bodyOffset` is the byte offset just after the opening brace of a block body, or null when the body is an expression or the function is a generator, whose body runs at its first `next()` and not when it is called; `users` are the other files of the scope that refer to the function.

import fs from 'node:fs'

import { compare, isTestOrDeclaration, openService, readOptions } from './service.mjs'

const options = readOptions()
const { ts, files, scope, absolute, relative, service, program } = openService(options)

/** The offset in bytes, as the file is stored, of a position in the text TypeScript read. */
function byteOffsetter(source) {
  const bytes = fs.readFileSync(source.fileName)
  const bom = bytes.length >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf ? 3 : 0
  if (bytes.toString('utf8', bom) !== source.text) throw new Error(`${relative(source.fileName)} is not UTF-8, so its offsets are not bytes`)
  return (position) => bom + Buffer.byteLength(source.text.slice(0, position), 'utf8')
}

/** The exported functions a statement declares: `export function f` and `export const f = (…) =>` or `= function`, never a default export. A generator is marked as one: calling it runs none of its body. */
function* exportedFunctions(statement) {
  const modifiers = ts.canHaveModifiers(statement) ? (ts.getModifiers(statement) ?? []) : []
  const has = (kind) => modifiers.some((modifier) => modifier.kind === kind)
  if (!has(ts.SyntaxKind.ExportKeyword) || has(ts.SyntaxKind.DefaultKeyword)) return
  if (ts.isFunctionDeclaration(statement)) {
    if (statement.name && statement.body) yield { name: statement.name, body: statement.body, generator: statement.asteriskToken !== undefined }
  } else if (ts.isVariableStatement(statement) && statement.declarationList.flags & ts.NodeFlags.Const) {
    for (const declaration of statement.declarationList.declarations) {
      const init = declaration.initializer
      if (ts.isIdentifier(declaration.name) && init && (ts.isArrowFunction(init) || ts.isFunctionExpression(init))) {
        yield { name: declaration.name, body: init.body, generator: init.asteriskToken !== undefined }
      }
    }
  }
}

const functions = []
for (const file of files) {
  if (isTestOrDeclaration(file)) continue
  const source = program.getSourceFile(absolute(file))
  let offset
  for (const statement of source.statements) {
    for (const { name, body, generator } of exportedFunctions(statement)) {
      offset ??= byteOffsetter(source)
      const position = name.getStart(source)
      const users = new Set()
      for (const symbol of service.findReferences(source.fileName, position) ?? []) {
        for (const reference of symbol.references) {
          const other = relative(reference.fileName)
          if (other !== file && scope.has(other)) users.add(other)
        }
      }
      functions.push({
        file,
        name: name.text,
        line: source.getLineAndCharacterOfPosition(position).line + 1,
        bodyOffset: ts.isBlock(body) && !generator ? offset(body.getStart(source) + 1) : null,
        users: [...users].sort(compare),
      })
    }
  }
}
functions.sort((a, b) => compare(a.file, b.file) || compare(a.name, b.name) || a.line - b.line)

process.stdout.write(JSON.stringify({ typescript: ts.version, files: files.length, functions }))
