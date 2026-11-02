vi.mock('./lib', () => ({}))
vi.unmock('./lib')

const lib = await import('./lib')
