const table: Record<string, number> = {};
const list: number[] = [];
const seen = new Set<string>();
const nested = { deep: { items: new Map<string, number>() } };
const untouched = new Map<string, number>();
let total = 0;
let current = 0;
let first = 0;
let second = 0;
let shorthand = 0;
let doomed: Record<string, number> = {};

function assignMember(key: string) {
  table[key] = 1;
}

function pushes() {
  list.push(1);
}

function adds(key: string) {
  seen.add(key);
}

function deep() {
  nested.deep.items.set('a', 1);
}

function reads(key: string) {
  return untouched.get(key);
}

function compound() {
  total += 1;
}

function loops(items: number[]) {
  for (current of items) {
    console.log(current);
  }
}

function destructures() {
  [first, second] = [1, 2];
}

function shorthands(source: { shorthand: number }) {
  ({ shorthand } = source);
}

function removes(key: string) {
  delete doomed[key];
}
