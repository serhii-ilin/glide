export function createCanvas() {
  return {
    getContext() {
      return null;
    },
    toDataURL() {
      return "";
    },
  };
}

export default { createCanvas };