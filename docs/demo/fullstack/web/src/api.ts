export async function listUsers() {
  const response = await fetch("/api/users");
  return response.json();
}
