import { Kafka } from 'kafkajs';

const kafka = new Kafka({ brokers: [] });
const producer = kafka.producer();

export const write = () => producer.send({ topic: 'u', messages: [] });
