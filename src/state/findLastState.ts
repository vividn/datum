import { DateTime } from "luxon";
import { DatumTime } from "../time/datumTime";
import { inferType } from "../utils/inferType";
import { getActiveState } from "./getActiveState";
import { DatumState } from "./normalizeState";

export async function getLastState({
  db,
  field,
  lastState,
  time,
}: {
  db: PouchDB.Database;
  field?: string;
  lastState?: DatumState;
  time?: DateTime | DatumTime | string;
}): Promise<DatumState | undefined> {
  if (lastState !== undefined) {
    return lastState === "string"
      ? (inferType(lastState) as DatumState)
      : lastState;
  } else if (time !== undefined && field !== undefined) {
    return await getActiveState(db, field, time);
  }
}
