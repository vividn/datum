// eslint-disable-next-line @typescript-eslint/explicit-module-boundary-types
export const createOrAppend = (
  existing: any[] | any,
  toAppend: any
): any[] | any => {
  if (existing === undefined) {
    return toAppend;
  }
  const existingArr = Array.isArray(existing ?? [])
    ? existing ?? []
    : [existing];
  existingArr.push(toAppend);
  return existingArr;
};
