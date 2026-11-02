import { OrderService } from './service';
import { OtherService } from './other';
import { Repository, User } from './repository';

class Runner {
  static start() {}
  static stop() {}
}

export function refund(id: string) {
  new OrderService().cancel(id);
}

export function refundLater(id: string) {
  const service = new OrderService();
  service.cancel(id);
}

export function refundWith(service: OrderService, id: string) {
  service.cancel(id);
}

export function pass(service: OrderService) {
  run(service);
}

export function keep() {
  const service = new OrderService();
  run(service);
}

export function create() {
  OrderService.create();
}

export function begin() {
  Runner.start();
}

export function load(repo: Repository<User>) {
  repo.find();
}

export function twice() {
  const a = new OrderService();
  const a = new OtherService();
  a.cancel();
}
