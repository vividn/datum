import {
  DatumData,
  DatumMetadata,
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
    data = payload as DatumData;
  }

  if (meta === undefined && typeof data["_id"] === "string") {
    // in no metadata mode, a manually specified _id takes precedence
    return data["_id"];
  }
  if (meta?.idStructure && idStructure && meta.idStructure !== idStructure) {
    throw new IdError("idStructure in meta and argument do not match");
  }

  idStructure ??= meta?.idStructure;

  if (idStructure === undefined) {
    if (payload._id !== undefined) {
      return payload._id;
    }
    throw new IdError("Cannot determine the id");
  }

  // For backwards compatibility, check if the id structure already starts with field: (to avoid double field prefixing)
  idStructure = idStructure.replace(/^%field%:/, "");

  // Generate the main part of the ID
  const mainId = interpolateFields({ data, meta, format: idStructure });

  // Add field partition if available
  if (data.field) {
    return `${data.field}:${mainId}`;
  }

  return mainId;
};
