type Left = { left: true };
type Right = { right: true };

export function joined(both: Left & Right) {
  return both.tenant.findFirst();
}

export function inline(pair: { first: true } & { second: true }) {
  return pair.tenant.findFirst();
}
