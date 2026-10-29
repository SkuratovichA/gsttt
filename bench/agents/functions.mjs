// The exported functions of a scope of a TypeScript checkout, with the files that refer to each of them: the candidates of `cargo xtask agents questions`.
//
//   node bench/agents/functions.mjs --tools <bench/tools/node> --checkout <dir> --scope <dir> --tsconfig <file>
//
// The answers come from the TypeScript language service (`findReferences`), not from gsttt, so that no tool under test marks its own work. The TypeScript is the one installed in `--tools`: the copy of the checkout and a global one are other versions.
// The program is every tracked TypeScript file of the scope with the compiler options of the tsconfig. A language service that cannot follow an import misses the references behind it, so the script refuses to answer while a relative import or a path of the tsconfig does not resolve to a file.
// Prints one JSON object on standard output, sorted so that the same checkout gives the same bytes:
//   { typescript, files, functions: [{ file, name, line, bodyOffset, users }] }
// `bodyOffset` is the byte offset just after the opening brace of a block body, or null when the body is an expression or the function is a generator, whose body runs at its first `next()` and not when it is called; `users` are the other files of the scope that refer to the function.

import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import { createRequire } from 'node:module'
import path from 'node:path'
import { parseArgs } from 'node:util'

const compare = (a, b) => (a < b ? -1 : a > b ? 1 : 0)

/** TypeScript from `<tools>/node_modules`, and nowhere else. */
function loadTypeScript(tools) {
  const require = createRequire(path.join(tools, 'package.json'))
  const entry = require.resolve('typescript')
  const installed = path.join(tools, 'node_modules') + path.sep
  if (!entry.startsWith(installed)) {
    throw new Error(`typescript resolves to ${entry}, outside ${installed}; install the tools with \`cargo xtask agents questions\``)
  }
  return require('typescript')
}

/** The compiler options of a tsconfig, less what only fails when `node_modules` is not installed. */
function compilerOptions(ts, file) {
  const read = ts.readConfigFile(file, ts.sys.readFile)
  if (read.error) throw new Error(ts.flattenDiagnosticMessageText(read.error.messageText, '\n'))
  // Only the options are of use: the files come from git, so the directories are not walked.
  const host = { ...ts.sys, readDirectory: () => [] }
  const parsed = ts.parseJsonConfigFileContent(read.config, host, path.dirname(file), undefined, file)
  // 18003 is "no inputs were found", which is what the stubbed `readDirectory` makes of every tsconfig.
  const errors = parsed.errors.filter((error) => error.code !== 18003)
  if (errors.length > 0) {
    throw new Error(`${file}: ${errors.map((error) => ts.flattenDiagnosticMessageText(error.messageText, '\n')).join('; ')}`)
  }
  const { types, typeRoots, plugins, ...options } = parsed.options
  return options
}

/** The files the `paths` of the tsconfig map `specifier` to; none when no key matches. */
function mappedFiles(specifier, settings) {
  const base = settings.baseUrl ?? settings.pathsBasePath
  const files = []
  for (const [pattern, targets] of Object.entries(settings.paths ?? {})) {
    const star = pattern.indexOf('*')
    const [prefix, suffix] = star < 0 ? [pattern, null] : [pattern.slice(0, star), pattern.slice(star + 1)]
    if (suffix === null ? specifier !== pattern : specifier.length < prefix.length + suffix.length || !specifier.startsWith(prefix) || !specifier.endsWith(suffix)) continue
    const captured = suffix === null ? '' : specifier.slice(prefix.length, specifier.length - suffix.length)
    files.push(...targets.map((target) => path.resolve(base, target.replace('*', captured))))
  }
  return files
}

/** Extensions of the files that imports name without being code. */
const NOT_CODE = new Set(
  '.css .scss .sass .less .stylus .pcss .postcss .svg .png .jpg .jpeg .gif .webp .avif .ico .bmp .woff .woff2 .ttf .otf .eot .mp3 .mp4 .webm .ogg .wav .txt .md .mdx .html .vue .svelte .wasm .node'.split(' '),
)

/** Whether `specifier` names a file that is not code: a query such as `?raw`, a stylesheet, an image or a font by its extension, or any file that exists but TypeScript does not read. The language service cannot follow such an import, and no reference can hide behind one. */
function namesNoCode(specifier, candidates) {
  if (/[?#]/.test(specifier) || NOT_CODE.has(path.extname(specifier).toLowerCase())) return true
  return candidates.some((file) => fs.existsSync(file) && fs.statSync(file).isFile())
}

/** Whether a file of the scope is a test or a declaration file, which no question is about. */
function isTestOrDeclaration(file) {
  return file.includes('__tests__/') || file.includes('.spec.') || file.includes('.test.') || /\.d\.[cm]?ts$/.test(file)
}

const { values: options } = parseArgs({
  options: { tools: { type: 'string' }, checkout: { type: 'string' }, scope: { type: 'string' }, tsconfig: { type: 'string' } },
})
for (const name of ['tools', 'checkout', 'scope', 'tsconfig']) {
  if (!options[name]) throw new Error(`--${name} is required`)
}

const ts = loadTypeScript(fs.realpathSync(path.resolve(options.tools)))
const checkout = fs.realpathSync(path.resolve(options.checkout))
const absolute = (file) => path.join(checkout, file)
const relative = (file) => path.relative(checkout, file)

const tracked = execFileSync('git', ['ls-files', '-z', '--', options.scope], { cwd: checkout, encoding: 'utf8', maxBuffer: 1 << 28 })
const files = tracked
  .split('\0')
  .filter((file) => /\.(ts|tsx|mts|cts)$/.test(file))
  .sort(compare)
if (files.length === 0) throw new Error(`no tracked TypeScript file under ${options.scope} in ${checkout}`)
const scope = new Set(files)

const compilerSettings = compilerOptions(ts, absolute(options.tsconfig))
const host = {
  getCompilationSettings: () => compilerSettings,
  getScriptFileNames: () => files.map(absolute),
  getScriptVersion: () => '1',
  getScriptSnapshot: (file) => {
    const text = ts.sys.readFile(file)
    return text === undefined ? undefined : ts.ScriptSnapshot.fromString(text)
  },
  getCurrentDirectory: () => checkout,
  getDefaultLibFileName: (settings) => ts.getDefaultLibFilePath(settings),
  fileExists: ts.sys.fileExists,
  readFile: ts.sys.readFile,
  readDirectory: ts.sys.readDirectory,
  directoryExists: ts.sys.directoryExists,
  getDirectories: ts.sys.getDirectories,
  realpath: ts.sys.realpath,
  useCaseSensitiveFileNames: () => ts.sys.useCaseSensitiveFileNames,
}
const service = ts.createLanguageService(host, ts.createDocumentRegistry(ts.sys.useCaseSensitiveFileNames, checkout))
const program = service.getProgram()

// Every relative import and every import a path of the tsconfig maps must resolve to a file.
const unresolved = []
const cache = ts.createModuleResolutionCache(checkout, (file) => file, compilerSettings)
for (const file of files) {
  const source = program.getSourceFile(absolute(file))
  const imports = ts.preProcessFile(source.text, true, true).importedFiles
  for (const { fileName: specifier, pos } of imports) {
    const relativeImport = /^\.\.?(\/|$)/.test(specifier)
    const mapped = mappedFiles(specifier, compilerSettings)
    if (!relativeImport && mapped.length === 0) continue
    if (ts.resolveModuleName(specifier, source.fileName, compilerSettings, host, cache, undefined, source.impliedNodeFormat).resolvedModule) continue
    const candidates = relativeImport ? [path.resolve(path.dirname(source.fileName), specifier)] : mapped
    if (namesNoCode(specifier, candidates)) continue
    unresolved.push(`${file}:${source.getLineAndCharacterOfPosition(pos).line + 1}: ${specifier}`)
  }
}
if (unresolved.length > 0) {
  throw new Error(`${unresolved.length} imports of ${options.scope} do not resolve to a file:\n${unresolved.sort(compare).join('\n')}`)
}

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
