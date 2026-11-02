process.env[process.env.TARGET] = 'v';
export function set(name: string) { process.env[name] = 'v'; }
