import {
  DatumData,
  DatumMetadata,
  EitherPayload,
  isDatumPayload,
} from "../documentControl/DatumDocument";
import { IdError } from "../errors";
import { splitRawAndFields } from "./splitRawAndFields";
import { GenericObject } from "../GenericObject";

type assembleIdType = {
  payload: EitherPayload;
  idStructure?: string;
};
export const assembleId = function ({
  payload,
  idStructure,
}: assembleIdType): string {
  let data: DatumData;
  let meta: DatumMetadata | undefined;
  if (isDatumPayload(payload)) {
    data = payload.data;
    meta = payload.meta;
  } else {
    data = payload as DatumData;
  }

  if (meta === undefined && data["_id"] !== undefined) {
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

  const rawEvenFieldOdd = splitRawAndFields(structure);

  const interpolatedFields = rawEvenFieldOdd
    .reduce((combined: string[], strPart: string, index: number) => {
      if (index % 2 === 0) {
        // raw ids get appended directly
        combined.push(strPart);
        return combined;
      }

      // Keys that start with '?' are pulled from meta instead
      let key: string, sourceObject: DatumData | DatumMetadata | undefined;
      if (strPart.startsWith("?")) {
        key = strPart.slice(1);
        sourceObject = meta;
      } else if (strPart.startsWith(String.raw`\?`)) {
        key = strPart.slice(1);
        sourceObject = data;
      } else {
        key = strPart;
        sourceObject = data;
      }

      // retrieve deeper values with dot notation
      const extractedValue = key
        .split(".")
        .reduce(
          (o, subKey) => (o as undefined | GenericObject)?.[subKey],
          sourceObject
        );
      if (extractedValue !== undefined) {
        const valueAsString =
          typeof extractedValue === "string"
            ? extractedValue
            : JSON.stringify(extractedValue);
        combined.push(valueAsString);
      }
      return combined;
    }, [])
    .join("");

  return interpolatedFields;
};
