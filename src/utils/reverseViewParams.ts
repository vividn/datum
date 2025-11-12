import { QueryOptions } from "./utilityTypes.js";

export function reverseViewParams(params: QueryOptions): QueryOptions {
  return {
    ...params,
    descending: !params.descending,
    startkey: params.endkey,
    endkey: params.startkey,
  };
}
