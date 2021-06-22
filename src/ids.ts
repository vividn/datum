import { DatumData, DatumMetadata } from "./documentControl/DatumDocument";
import { GenericObject } from "./GenericObject";
import deepGet from "lodash.get";
import deepSet from "lodash.set";
import deepUnset from "lodash.unset";
import { IdError } from "./errors";
import { defaults } from "./input/defaults";

export type buildIdStructureType = {
  idParts: string | string[];
  partition?: string | string[];
  delimiter?: string;
};
export const buildIdStructure = function ({
  idParts,
  partition,
  delimiter = defaults.idDelimiter,
}: buildIdStructureType): string {
  // % is reserved for field names, escape it
  delimiter = delimiter === "%" ? "\\%" : delimiter;

  const partitionAndId =
    partition === undefined ? [idParts] : [partition, idParts];
  const structurizedPartitionId = partitionAndId.map((idOrPartition) => {
    const inputArray =
      typeof idOrPartition === "string" ? [idOrPartition] : idOrPartition;

    // for convenience, user can use just "%fieldName" rather than the full "%fieldName%", this adds the missing percent at the end
    const appendedTrailingPercent = inputArray.map((idComponent) => {
      // skip escaped percents
      const percentCount = (idComponent.match(/(?<!\\)%/g) || []).length;
      return percentCount % 2 === 0 ? idComponent : idComponent + "%";
    });

    return appendedTrailingPercent.join(delimiter);
  });

  return structurizedPartitionId.join(":");
};

export const destructureIdKeys = (
  datumData: DatumData,
  idStructure?: string
): { onlyFields: GenericObject; noFields: GenericObject } => {
  const noFields = JSON.parse(JSON.stringify(datumData));
  const onlyFields = {} as GenericObject;

  idStructure = idStructure ?? datumData.meta?.idStructure;
  if (idStructure === undefined) {
    return { onlyFields, noFields };
  }

  const fieldNames = splitRawAndFields(idStructure).filter(
    (_, index) => index % 2 === 1
  );

  fieldNames.forEach((fieldName) => {
    const extractedValue = deepGet(datumData, fieldName);
    deepSet(onlyFields, fieldName, extractedValue);
    deepUnset(noFields, fieldName);
  });

  return { onlyFields, noFields };
};

export const splitRawAndFields = (str: string): string[] => {
  // split apart and also replace the escaped %s with normal percents
  return str
    .replace(/(?<!\\)%/g, "\xff\x00")
    .replace(/\\%/g, "%")
    .split("\xff\x00");
};

type assembleIdType = {
  data: DatumData;
  meta?: DatumMetadata;
  idStructure?: string;
};
export const assembleId = function ({
  data,
  meta,
  idStructure,
}: assembleIdType): string {
  if (meta === undefined && data["_id"] !== undefined) {
    // in no metadata mode, a manually specified _id takes precedence
    return data["_id"];
  }
  if (meta?.idStructure && idStructure && meta.idStructure !== idStructure) {
    throw new IdError("idStructure in meta and argument do not match");
  }

  const structure = idStructure ?? meta?.idStructure;

  if (structure === undefined) {
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

export const defaultIdComponents = ({
  data,
  hasOccurTime,
}: {
  data: DatumData;
  hasOccurTime: boolean;
}): { defaultIdParts: string[]; defaultPartitionParts?: string[] } => {
  const defaultIdParts = hasOccurTime
    ? ["%?occurTime%"]
    : Object.keys(data).map((key) => `%${key.replace(/%/g, String.raw`\%`)}%`);
  const defaultPartitionParts = "field" in data ? ["%field%"] : undefined;
  return { defaultIdParts, defaultPartitionParts };
};
