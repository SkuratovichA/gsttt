process.env.A = 'x';
process.env['B'] ??= 'y';
delete process.env.C;
vi.stubEnv('D', 'z');
({ E: process.env.F } = values);
