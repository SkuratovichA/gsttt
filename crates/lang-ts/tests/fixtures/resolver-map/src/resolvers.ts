declare function list(): string[];
declare function cancel(): string;

export const resolvers = {
  Query: {
    bookings: () => list(),
  },
  Mutation: {
    cancel,
  },
};
