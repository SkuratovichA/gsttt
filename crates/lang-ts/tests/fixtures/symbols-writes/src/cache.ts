const cache = new Map<string, number>();

function fill(k: string) {
  cache.set(k, 1);
}

function read(k: string) {
  return cache.get(k);
}

let count = 0;

function bump() {
  count++;
}
