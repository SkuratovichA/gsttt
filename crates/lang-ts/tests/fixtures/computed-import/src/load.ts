export async function load(name: string) {
  return await import(name);
}
