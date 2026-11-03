const config = { a: 1, b: 2 };

export function take() {
  const { a } = config;
  return a;
}
