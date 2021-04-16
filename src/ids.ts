import { DatumDocument } from "./documentControl/DatumDocument";
import { GenericObject } from "./GenericObject";
import deepGet from "lodash.get";
import deepSet from "lodash.set";
import deepUnset from "lodash.unset";

export const assembleId = function ({
  idPart = "%meta.occurTime%",
  delimiter = "__",
  partition = "%field%",
  payload,
}: {
  idPart?: string | string[];
  delimiter?: string;
  partition?: string | string[];
  payload: GenericObject;
}): { id: string; structure: string } {
  if ("_id" in payload) {
    return { id: payload["_id"], structure: payload["_id"] };
  }

  const partitionStructure = buildIdStructure(partition, delimiter);
  const idStructure = buildIdStructure(idPart, delimiter);

  const partitionSection = idFromStructure(partitionStructure, payload);
  const idSection = idFromStructure(idStructure, payload);
  if (partitionSection.length > 0) {
    const structure = `${partitionStructure}:${idStructure}`;
    const id = `${partitionSection}:${idSection}`;
    return { id, structure };
  } else {
    return { id: idSection, structure: idStructure };
  }
};

const buildIdStructure = function (
  idOrPartition: string | string[],
  delimiter: string
): string {
  // % is reserved for field names, escape it
  delimiter = delimiter === "%" ? "\\%" : delimiter;
  const inputArray =
    typeof idOrPartition === "string" ? [idOrPartition] : idOrPartition;

  // for convienience, user can use just "%fieldName" rather than the full "%fieldName%", this adds the missing percent at the end
  const appendedTrailingPercent = inputArray.map((idComponent) => {
    // skip escaped percents
    const percentCount = (idComponent.match(/(?<!\\)%/g) || []).length;
    return percentCount % 2 === 0 ? idComponent : idComponent + "%";
  });

  return appendedTrailingPercent.join(delimiter);
};

export const destructureIdKeys = (
  doc: DatumDocument,
  idStructure?: string
): { onlyFields: GenericObject; noFields: GenericObject } => {
  const noFields = JSON.parse(JSON.stringify(doc));
  const onlyFields = {} as GenericObject;

  idStructure = idStructure ?? doc.meta?.idStructure;
  if (idStructure === undefined) {
    return { onlyFields, noFields };
  }

  const fieldNames = splitRawAndFields(idStructure).filter(
    (_, index) => index % 2 === 1
  );

  fieldNames.forEach((fieldName) => {
    const extractedValue = deepGet(doc, fieldName);
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

export const idFromStructure = function (
  structure: string,
  payload: GenericObject
): string {
  const rawEvenFieldOdd = splitRawAndFields(structure);

  const interpolatedFields = rawEvenFieldOdd
    .reduce((combined: string[], strPart: string, index: number) => {
      if (index % 2 === 0) {
        combined.push(strPart);
      } else {
        // retrieve deeper values with dot notation
        const extractedValue = strPart
          .split(".")
          .reduce((o, i) => (o === undefined ? undefined : o[i]), payload);
        if (extractedValue !== undefined) {
          const valueAsString =
            typeof extractedValue === "string"
              ? extractedValue
              : JSON.stringify(extractedValue);
          combined.push(valueAsString);
        }
      }
      return combined;
    }, [])
    .join("");

  return interpolatedFields;
};
