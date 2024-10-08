import { DatumData, DatumMetadata } from "../documentControl/DatumDocument";
import { splitRawAndFields } from "../ids/splitRawAndFields";
import { humanTime } from "../time/humanTime";
import { isDatumTime } from "../time/datumTime";
import { JsonObject, JsonType } from "./utilityTypes";
import isPlainObject from "lodash.isplainobject";

export function interpolateFields({
  data,
  meta,
  format,
  useHumanTimes = false,
}: {
  data: DatumData;
  meta?: DatumMetadata;
  format: string;
  useHumanTimes?: boolean;
}): string {
  const rawEvenFieldOdd = splitRawAndFields(format);

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
      const extractedValue: JsonType | undefined = key
        .split(".")
        .reduce((o: JsonType | undefined, subKey) => {
          if (isPlainObject(o) && subKey in (o as JsonObject)) {
            return (o as JsonObject)[subKey];
          }
        }, sourceObject);
      if (extractedValue !== undefined) {
        if (isDatumTime(extractedValue)) {
          if (useHumanTimes) {
            combined.push(humanTime(extractedValue) ?? extractedValue.utc);
          } else {
            combined.push(extractedValue.utc);
          }
          return combined;
        }
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
}
