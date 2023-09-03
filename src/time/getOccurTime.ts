import {
  DatumData,
  EitherDocument,
  isDatumDocument,
} from "../documentControl/DatumDocument";
import { DateTime, FixedOffsetZone } from "luxon";

export function getOccurTime(
  doc: EitherDocument | DatumData
): DateTime | undefined {
  const data = isDatumDocument(doc as EitherDocument)
    ? (doc.data as DatumData)
    : (doc as DatumData);
  const occurTime = data.occurTime
    ? DateTime.fromISO(data.occurTime, {
        zone: data.occurUtcOffset
          ? FixedOffsetZone.instance(60 * data.occurUtcOffset)
          : undefined,
      })
    : undefined;
  return occurTime;
}
