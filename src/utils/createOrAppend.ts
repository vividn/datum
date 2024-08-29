export function createOrAppend(
  existing: unknown[] | unknown,
  toAppend: unknown,
): unknown[] | unknown {
  if (existing === undefined) {
    return toAppend;
  }
  const arr = Array.isArray(existing) ? existing : [existing];
  arr.push(toAppend);
  return arr;
}
