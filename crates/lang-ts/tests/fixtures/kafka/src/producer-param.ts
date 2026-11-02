import { Producer } from 'kafkajs';

export const write = (producer: Producer, messages: string[]) =>
  producer.send({ topic: 't', messages });
