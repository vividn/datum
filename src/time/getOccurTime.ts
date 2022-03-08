import {
  EitherDocument,
  isDatumDocument,
} from "../documentControl/DatumDocument";
import { DateTime, FixedOffsetZone } from "luxon";

export function getOccurTime(doc: EitherDocument): DateTime | undefined {
  if (isDatumDocument(doc)) {
    const data = doc.data;
    const meta = doc.meta;
    const occurTime = data.occurTime
      ? DateTime.fromISO(data.occurTime, {
          zone: meta.utcOffset
            ? FixedOffsetZone.instance(60 * meta.utcOffset)
            : undefined,
        })
      : undefined;
    return occurTime;
  }
  const occurTime = doc.occurTime ? DateTime.fromISO(doc.occurTime) : undefined;
  return occurTime;
}
