import { Left, MailQueue, Right, Special } from './queues';

class Spellings {
  #queue: MailQueue;
  constructor(
    private readonly mail: MailQueue,
    private readonly both: Left & Right,
  ) {}
  optional() {
    this.mail?.push();
  }
  nonNull() {
    this.mail!.push();
  }
  cast() {
    (this.mail as Special).push();
  }
  quoted() {
    this['mail'].push();
  }
  bound() {
    this.mail.push.bind(null);
  }
  destructured() {
    const { push } = this.mail;
    push();
  }
  alias() {
    const queue = this.mail;
    queue.push();
  }
  intersection() {
    this.both.send();
  }
  privateCast() {
    (this.#queue as Special).push();
  }
  privateNonNull() {
    this.#queue!.push();
  }
}
