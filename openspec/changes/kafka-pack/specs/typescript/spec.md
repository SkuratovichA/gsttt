## ADDED Requirements

### Requirement: A call rule that lists receiver types
A call rule MAY list `receiver_types`. It SHALL read a call only when the receiver is a name declared in the file with one of them, by the same rules as the root types of a chain rule, or `this.<name>` for a constructor parameter property. A rule that lists none SHALL read any receiver.

#### Scenario: A call rule reads only a receiver of a listed type
- **WHEN** a project pack has a call rule on `*.*.emit` with `receiver_types = ["EventEmitter2"]`, and one file calls `this.emitter.emit('change', x)` on a constructor parameter property of that type and another calls `this.kafka.emit('user.created', x)` on a `ClientKafka`
- **THEN** only the first calls `queue change`

### Requirement: Kafka consumers
With the `kafka` pack, active when the project depends on `kafkajs`, `@EventPattern(topic)` and `@MessagePattern(topic)` SHALL serve `queue <topic>`, and `subscribe({ topic })` on a name declared with `Consumer` SHALL serve it.

#### Scenario: A message pattern serves its topic
- **WHEN** a class has `@EventPattern('user.created')` and `@MessagePattern('user.get')`
- **THEN** the file serves `queue user.created` and `queue user.get`

#### Scenario: A consumer subscribed to a topic serves it
- **WHEN** a parameter `consumer: Consumer` calls `consumer.subscribe({ topic: 't' })`
- **THEN** the file serves `queue t`

#### Scenario: A project without kafkajs has no Kafka rules
- **WHEN** a project does not depend on `kafkajs` and a class has `@EventPattern('user.created')`
- **THEN** the file serves no contract and has no unread site

### Requirement: Kafka producers
With the `kafka` pack, `emit`, `send` and `emitMessage` on a name declared with `ClientKafka`, `ClientProxy`, `KafkaProducerService` or `Producer` SHALL call `queue <topic>`, the topic being the first argument or its `topic` property. A name that the file also binds to another kind, and a name with no declared type, SHALL be no producer.

#### Scenario: A client held in a constructor parameter property calls its topic
- **WHEN** a constructor has `private readonly kafka: ClientKafka` and a method calls `this.kafka.emit('user.created', x)`
- **THEN** the file calls `queue user.created`

#### Scenario: A client held in a class property calls its topic
- **WHEN** a class declares `@Inject('KAFKA') private readonly kafka: ClientKafka;` and calls `this.kafka.emit('user.deleted', {})`
- **THEN** the file calls `queue user.deleted`

#### Scenario: A producer service calls its topic
- **WHEN** a class has `private readonly kafkaProducerService: KafkaProducerService` and calls `this.kafkaProducerService.emitMessage('t', m)`
- **THEN** the file calls `queue t`

#### Scenario: A producer received as a parameter calls its topic
- **WHEN** a function has the parameter `producer: Producer` and calls `producer.send({ topic: 't', messages })`
- **THEN** the file calls `queue t`

#### Scenario: An event emitter is no producer
- **WHEN** a class has `private readonly emitter: EventEmitter2` and calls `this.emitter.emit('change', x)`
- **THEN** the file calls no contract and has no unread site

#### Scenario: A property of two types in two classes of one file is no producer
- **WHEN** a file has one class with `constructor(private readonly sender: ClientKafka) {}` and another with `constructor(private readonly sender: EventEmitter2) {}`, and both call `this.sender.emit`
- **THEN** the file calls no contract

#### Scenario: A name bound to two kinds in one file is no producer
- **WHEN** a file has `client: Producer` in one function and `client: EventEmitter2` in another, and both call `client.send` or `client.emit`
- **THEN** the file calls no contract
