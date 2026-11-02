vi.mock('./lib', async (importOriginal) => ({ ...(await importOriginal()) }))
jest.mock('./lib', async (original) => { const lib = await original(); return lib })
vi.mock('./lib', async ({ lib }) => ({ lib }))
