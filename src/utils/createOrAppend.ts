/* eslint-disable @typescript-eslint/explicit-module-boundary-types */
export function createOrAppend(
  existing: any[] | any,
  toAppend: any,
): any[] | any {
  if (existing === undefined) {
    return toAppend;
  }
  const arr = Array.isArray(existing) ? existing : [existing];
  arr.push(toAppend);
  return arr;
}
/* eslint-enable @typescript-eslint/explicit-module-boundary-types */
