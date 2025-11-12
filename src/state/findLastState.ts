import { DateTime } from "luxon";
import { DatumTime } from "../time/datumTime.js";
import { inferType } from "../utils/inferType.js";
import { getActiveState } from "./getActiveState.js";
import { DatumState } from "./normalizeState.js";

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
