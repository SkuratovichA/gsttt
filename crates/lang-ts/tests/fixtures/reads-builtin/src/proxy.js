const wrapped = new Proxy(load(), {});

function call() {
  wrapped.proxied();
  globalThis.held.hung();
}

function load() {}
