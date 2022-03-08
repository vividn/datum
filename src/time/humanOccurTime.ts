import { EitherDocument } from "../documentControl/DatumDocument";
import { getOccurTime } from "./getOccurTime";
import { humanTime } from "./humanTime";

export function humanOccurTime(doc: EitherDocument): string | undefined {
  const occurTime = getOccurTime(doc);
  return occurTime ? humanTime(occurTime) : undefined;
}
