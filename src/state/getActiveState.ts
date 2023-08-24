import { DateTime } from "luxon";
import { activeStateView, DatumState } from "./activeStateView";
import { now } from "../time/timeUtils";

export async function getActiveState(
  db: PouchDB.Database,
  field: string,
  time: DateTime = now()
): Promise<DatumState> {
  const utcTime = time.toUTC().toISO();
  if (utcTime === null) {
    throw new Error("bad time");
  }
  const viewResult = await db.query(activeStateView.name, {
    startkey: [field, time.toUTC().toISO()],
    descending: true,
    limit: 1,
  });

  if (viewResult.rows.length === 0) {
    return null;
  }
  const activeStateRow = viewResult.rows[0];
  if (activeStateRow.key[0] !== field) {
    return null;
  }
  return activeStateRow.value;
}
