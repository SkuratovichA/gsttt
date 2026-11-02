import { Consumer } from 'kafkajs';

export const listen = (consumer: Consumer) => consumer.subscribe({ topic: 't' });
