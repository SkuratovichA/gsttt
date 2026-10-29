vi.mock('./lib', async () => ({
  ...(await vi.importActual<typeof import('./lib')>('./lib')),
}));

vi.mock('./editor', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./editor')>()),
}));
