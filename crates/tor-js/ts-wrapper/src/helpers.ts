export function never(value: never): never {
  throw new Error(`Unexpected value: ${safeToString(value)}`);
}

function safeToString(value: unknown) {
  try {
    return JSON.stringify(value);
  } catch {}

  try {
    return `${value}`;
  } catch {}

  return '(string conversion failed)';
}
