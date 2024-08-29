import {
  DatumData,
  DatumMetadata,
  DatumPayload,
  EitherPayload,
  isDatumPayload,
} from "../documentControl/DatumDocument";
import { IdError } from "../errors";
import { interpolateFields } from "../utils/interpolateFields";

type assembleIdType = {
  payload: EitherPayload;
  idStructure?: string;
};

export const assembleId = function ({
  payload,
  idStructure,
}: assembleIdType): string {
  let data;
  let meta: DatumMetadata | undefined;
  if (isDatumPayload(payload)) {
    data = payload.data as DatumData;
    meta = payload.meta;
  } else {
    data = payload as DatumPayload;
  }

  if (meta === undefined && typeof data["_id"] === "string") {
    // in no metadata mode, a manually specified _id takes precedence
    return data["_id"];
  }
  if (meta?.idStructure && idStructure && meta.idStructure !== idStructure) {
    throw new IdError("idStructure in meta and argument do not match");
  }

  const structure = idStructure ?? meta?.idStructure;

  if (structure === undefined) {
    if (payload._id !== undefined) {
      return payload._id;
    }
    throw new IdError("Cannot determine the id");
  }

  return interpolateFields({ data, meta, format: structure });
};
