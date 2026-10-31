// The TypeScript language service over a scope of a checkout, which `functions.mjs` and `clients.mjs` both ask their questions of: one copy of how the program is built, so that the two cannot read a checkout differently.
//
// The answers come from the TypeScript language service, not from gsttt, so that no tool under test marks its own work. The TypeScript is the one installed in `--tools`: the copy of the checkout and a global one are other versions.
// The program is every tracked TypeScript file of the scope with the compiler options of the tsconfig. A language service that cannot follow an import misses the references behind it, so `openService` refuses while a relative import or a path of the tsconfig does not resolve to a file.

import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import { createRequire } from 'node:module'
import path from 'node:path'
import { parseArgs } from 'node:util'

export const compare = (a, b) => (a < b ? -1 : a > b ? 1 : 0)

/** A package from `<tools>/node_modules`, and nowhere else. */
export function loadTool(tools, name) {
  const require = createRequire(path.join(tools, 'package.json'))
  const entry = require.resolve(name)
  const installed = path.join(tools, 'node_modules') + path.sep
  if (!entry.startsWith(installed)) {
    throw new Error(`${name} resolves to ${entry}, outside ${installed}; install the tools with \`cargo xtask agents questions\``)
  }
  return require(name)
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
export function isTestOrDeclaration(file) {
  return file.includes('__tests__/') || file.includes('.spec.') || file.includes('.test.') || /\.d\.[cm]?ts$/.test(file)
}

/** The options both scripts take: `--tools <bench/tools/node> --checkout <dir> --scope <dir> --tsconfig <file>`. */
export function readOptions({ fields = false } = {}) {
  const { values } = parseArgs({
    options: { tools: { type: 'string' }, checkout: { type: 'string' }, scope: { type: 'string' }, tsconfig: { type: 'string' }, ...(fields ? { fields: { type: 'boolean' } } : {}) },
  })
  for (const name of ['tools', 'checkout', 'scope', 'tsconfig']) {
    if (!values[name]) throw new Error(`--${name} is required`)
  }
  return values
}

/**
 * The language service over the tracked TypeScript files of `options.scope`, or only of those below `options.program` when it is given: a scope may hold packages the tsconfig does not describe, whose imports it cannot resolve.
 * Returns `{ ts, tools, checkout, tracked, files, scope, absolute, relative, service, program }`: `tracked` is every file git tracks under the scope, `files` the TypeScript files of the program, sorted, and `scope` the set of those.
 */
export function openService(options, { program: below } = {}) {
  const tools = fs.realpathSync(path.resolve(options.tools))
  const ts = loadTool(tools, 'typescript')
  const checkout = fs.realpathSync(path.resolve(options.checkout))
  const absolute = (file) => path.join(checkout, file)
  const relative = (file) => path.relative(checkout, file)

  const tracked = execFileSync('git', ['ls-files', '-z', '--', options.scope], { cwd: checkout, encoding: 'utf8', maxBuffer: 1 << 28 })
    .split('\0')
    .filter((file) => file !== '')
  const inProgram = (file) => below === undefined || below === '.' || file.startsWith(`${below}/`)
  const files = tracked
    .filter((file) => /\.(ts|tsx|mts|cts)$/.test(file) && inProgram(file))
    .sort(compare)
  if (files.length === 0) throw new Error(`no tracked TypeScript file under ${below ?? options.scope} in ${checkout}`)

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
    throw new Error(`${unresolved.length} imports of ${below ?? options.scope} do not resolve to a file:\n${unresolved.sort(compare).join('\n')}`)
  }

  return { ts, tools, checkout, tracked, files, scope: new Set(files), absolute, relative, service, program }
}
