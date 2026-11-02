declare const cp: { exec: (command: string) => unknown };

export const list = () => cp.exec('ls -la');
