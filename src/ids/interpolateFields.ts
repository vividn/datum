import { DatumData, DatumMetadata } from "../documentControl/DatumDocument";
import { splitRawAndFields } from "./splitRawAndFields";
import { GenericObject } from "../GenericObject";
import { isIsoDateOrTime } from "../time/timeUtils";
import { humanTimeFromISO } from "../time/humanTime";

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
        const formattedString = isIsoDateOrTime(valueAsString)
          ? humanTimeFromISO(valueAsString)
          : valueAsString;
        combined.push(formattedString);
      }
      return combined;
    }, [])
    .join("");

  return interpolatedFields;
}
