export interface Provider {
  name: string;
  list(): string[];
  list(id: string): string[];
  [key: string]: unknown;
  ['computed'](): void;
}

export default interface Named {
  id(): number;
}

type Shape = {
  size(): number;
};
