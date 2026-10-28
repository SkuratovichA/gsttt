import gql from 'graphql-tag';

export const unrelated = 1;

export const Q = gql`query Bookings { bookings { id } }`;
