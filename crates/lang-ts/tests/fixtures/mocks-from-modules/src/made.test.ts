jest.setMock('./lib', {});
jest.createMockFromModule('./lib');
jest.genMockFromModule('./lib');
jest.unstable_mockModule('./lib', () => ({}));
jest.deepUnmock('./lib');
