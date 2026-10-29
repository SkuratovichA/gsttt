type Wide = Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide;

export function find(value: Wide) {
  return value.tenant.findFirst();
}
