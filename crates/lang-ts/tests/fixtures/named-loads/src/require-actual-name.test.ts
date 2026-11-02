const name = process.env.MODULE ?? './lib'

jest.mock('./lib', () => ({}))

const lib = jest.requireActual(name)
