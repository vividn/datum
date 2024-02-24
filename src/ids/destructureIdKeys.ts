import { DatumData } from "../documentControl/DatumDocument";
import { GenericObject } from "../GenericObject";
import deepGet from "lodash.get";
import deepSet from "lodash.set";
import deepUnset from "lodash.unset";
import { splitRawAndFields } from "./splitRawAndFields";

export function destructureIdKeys(
  datumData: DatumData,
  idStructure?: string
): { onlyFields: GenericObject; noFields: GenericObject } {
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
}
