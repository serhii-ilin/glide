export function connect() {
  throw new Error("net.connect is not available in the spike harness");
}

export default { connect };