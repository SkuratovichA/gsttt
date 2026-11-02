vi.mock('./lib')
vi.mock('./lib', factory)
jest.mock('./lib', { spy: true })
