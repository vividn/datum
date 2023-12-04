export function splitFirst(
  separator: string,
  str: string,
): [string] | [string, string] {
  const [first, ...eqSepValue] = str.split(separator);
  if (eqSepValue.length === 0) {
    return [first];
  }
  return [first, eqSepValue.join(separator)];
}
