const { A, B } = typeof window === 'undefined' ? require('./a') : {}
const lib = flag ? require('./b') : require('./c')
let Linter = null
let ns = null
function load() {
  Linter = require('./d').Linter
  ns = require('./g')
}
const pick = require('./e')[key]
const other = require('./f')[Symbol.iterator]
