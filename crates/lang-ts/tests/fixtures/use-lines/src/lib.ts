export function first(): void {}
export function second(): void {}
export class Builder {
  add(): Builder {
    return this;
  }
  build(): void {}
}
