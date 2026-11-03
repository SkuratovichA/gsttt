import { Service } from './service';

export function host() {
  abstract class Base {
    abstract service: Service;

    run(): void {
      this.service.update();
    }
  }
  return Base;
}
