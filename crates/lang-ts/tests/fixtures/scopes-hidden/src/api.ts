const BASE = '/api';

export function param(BASE: string) {
  return fetch(BASE);
}

export function shadow() {
  const BASE = '/local';
  return fetch(BASE);
}

export function mutable() {
  let BASE = '/mutable';
  return fetch(BASE);
}

export function destructured(options: { BASE: string }) {
  const { BASE } = options;
  return fetch(BASE);
}

export function hoisted(flag: boolean) {
  if (flag) {
    var BASE = '/inner';
  }
  return fetch(BASE);
}

export function typed() {
  let handler: (BASE: string) => void;
  return fetch(BASE);
}

export function callback(done: (BASE: string) => void) {
  return fetch(BASE);
}

fetch(BASE);
