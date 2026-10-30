export class Lazy {
  async load() {
    return import('./target');
  }
}
