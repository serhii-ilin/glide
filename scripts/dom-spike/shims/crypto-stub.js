export function randomBytes(n) {
  const out = new Uint8Array(n);
  for (let i = 0; i < n; i++) {
    out[i] = (Math.random() * 256) | 0;
  }
  return out;
}

export function randomUUID() {
  return "00000000-0000-4000-8000-000000000000";
}

export default { randomBytes, randomUUID };