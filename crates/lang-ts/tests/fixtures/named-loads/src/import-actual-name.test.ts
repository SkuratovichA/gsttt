const name = process.env.MODULE ?? './lib'

vi.mock('./lib', () => ({}))

const lib = await vi.importActual(name)
