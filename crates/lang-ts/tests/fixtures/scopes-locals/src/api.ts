const BASE = '/api';

export function plain() {
  const path = '/a';
  return fetch(path);
}

export function built() {
  const base = BASE + '/x';
  const url = base + '/y';
  return fetch(url);
}

export function branches(flag: boolean) {
  if (flag) {
    const url = '/first';
    return fetch(url);
  } else {
    const url = '/second';
    return fetch(url);
  }
}

export function later() {
  const call = () => fetch(path);
  const path = '/later';
  return call;
}

export function outer() {
  const path = '/outer';
  function inner() {
    return fetch(path);
  }
  return inner();
}

export function routes() {
  const paths = { users: '/users', orders: '/orders' };
  fetch(paths.users);
  return fetch(paths.orders);
}
