import {
  DatumData,
  EitherDocument,
  isDatumDocument,
} from "../documentControl/DatumDocument";
import { DateTime } from "luxon";
import { datumTimeToLuxon } from "./timeUtils";

export function getOccurTime(
  doc: EitherDocument | DatumData
): DateTime | undefined {
  const data = isDatumDocument(doc as EitherDocument)
    ? (doc.data as DatumData)
    : (doc as DatumData);
  const occurTime = data.occurTime;
  return occurTime ? datumTimeToLuxon(occurTime) : undefined;
}
