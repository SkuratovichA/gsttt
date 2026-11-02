const name = process.env.MODULE ?? './lib'

jest.mock('./lib', () => ({}))

const lib = require(name)
