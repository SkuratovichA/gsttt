vi.mock('./lib', () => ({ lib: 2 }))
jest.mock('./lib', function () { return { lib: 3 } })
vi.mock('./lib', async (importOriginal) => ({ lib: 4 }))
