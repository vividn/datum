import { DatumState } from "../views/datumViews/activeStateView";
import { DateTime } from "luxon";
import { inferType } from "../utils/inferType";
import { getActiveState } from "./getActiveState";

export async function getLastState({
  db,
  field,
  lastState,
  time,
}: {
  db: PouchDB.Database;
  field?: string;
  lastState?: DatumState;
  time?: DateTime | string;
}): Promise<DatumState | undefined> {
  if (lastState !== undefined) {
    return lastState === "string"
      ? (inferType(lastState) as DatumState)
      : lastState;
  } else if (time !== undefined && field !== undefined) {
    return await getActiveState(db, field, time);
  }
}
