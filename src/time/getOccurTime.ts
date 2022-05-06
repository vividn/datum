import {
  EitherDocument,
  isDatumDocument,
} from "../documentControl/DatumDocument";
import { DateTime, FixedOffsetZone } from "luxon";

export function getOccurTime(doc: EitherDocument): DateTime | undefined {
  const data = isDatumDocument(doc) ? doc.data : doc;
  const occurTime = data.occurTime
    ? DateTime.fromISO(data.occurTime, {
        zone: data.occurUtcOffset
          ? FixedOffsetZone.instance(60 * data.occurUtcOffset)
          : undefined,
      })
    : undefined;
  return occurTime;
}
