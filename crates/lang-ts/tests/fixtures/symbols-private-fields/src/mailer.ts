import { JobQueue, MailQueue } from './services';

class Mailer {
  #queue: MailQueue;
  #jobs = new JobQueue();
  queue = 0;
  send() {
    this.#queue.push();
  }
  later() {
    this.#jobs.add();
  }
  count() {
    return this.queue;
  }
}
