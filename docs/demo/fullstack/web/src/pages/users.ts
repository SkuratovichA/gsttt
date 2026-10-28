import { listUsers } from "../api";

export async function renderUsers() {
  const users = await listUsers();
  document.body.textContent = `${users.length} users`;
}
