import { DatumData, DatumMetadata } from "../documentControl/DatumDocument";
import { splitRawAndFields } from "../ids/splitRawAndFields";
import { GenericObject } from "../GenericObject";
import { isDatumTime } from "../time/timeUtils";
import { humanFormattedTime } from "../output/output";

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
        if (isDatumTime(extractedValue)) {
          if (useHumanTimes) {
            combined.push(
              humanFormattedTime(extractedValue) ?? extractedValue.utc
            );
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
