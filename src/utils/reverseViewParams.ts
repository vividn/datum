export function reverseViewParams(
  params: PouchDB.Query.Options<any, any>,
): PouchDB.Query.Options<any, any> {
  return {
    ...params,
    descending: !params.descending,
    startkey: params.endkey,
    endkey: params.startkey,
  };
}
