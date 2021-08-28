/* eslint-disable @typescript-eslint/explicit-module-boundary-types */
export function createOrAppend(
  existing: any[] | any,
  toAppend: any
): any[] | any {
  if (existing === undefined) {
    return toAppend;
  }
  const existingArr = Array.isArray(existing ?? [])
    ? existing ?? []
    : [existing];
  existingArr.push(toAppend);
  return existingArr;
}
/* eslint-enable @typescript-eslint/explicit-module-boundary-types */
