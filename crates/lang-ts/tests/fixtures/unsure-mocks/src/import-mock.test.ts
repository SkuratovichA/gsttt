vi.mock('./lib', () => ({}))

const lib = await vi.importMock('./lib')
