export const a = process.env.A;
export const b = process.env['B'];
export const c = import.meta.env.VITE_C;
export const { D, E: renamed } = process.env;
