interface Base {
  base: true;
}

class Cache extends Map {}

interface Shape extends Base {}

export function cached(cache: Cache) {
  return cache.tenant.findFirst();
}

export function shaped(shape: Shape) {
  return shape.tenant.findFirst();
}
