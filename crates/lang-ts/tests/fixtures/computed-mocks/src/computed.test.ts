const path = './lib';
vi.mock(path);
jest.mock(`./${name}`);
vi.importActual('./' + name);
await import(name);
