const name = process.env.MODULE ?? './lib'

vi.mock('./lib', () => ({}))

const lib = await import(name)
