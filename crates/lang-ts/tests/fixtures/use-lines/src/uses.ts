import { Builder, first, second } from "./lib";

export function run(): void {
  first();
  second();
  new Builder()
    .add()
    .build();
}

first();
