export class Sender {
  constructor(private readonly kafkaProducerService: KafkaProducerService) {}

  send(message: string) {
    this.kafkaProducerService.emitMessage('t', message);
  }
}
