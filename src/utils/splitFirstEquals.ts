export const splitFirstEquals = (str: string): [string, string | undefined] => {
  const [first, ...eqSepValue] = str.split("=");
  if (eqSepValue.length === 0) {
    return [first, undefined];
  }
  return [first, eqSepValue.join("=")];
};
