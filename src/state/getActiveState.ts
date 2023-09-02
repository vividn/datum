import { DateTime } from "luxon";
import { activeStateView, DatumState } from "./activeStateView";
import { now } from "../time/timeUtils";
import { DatumViewMissingError, isCouchDbError } from "../errors";
import { parseTimeStr } from "../time/parseTimeStr";

export async function getActiveState(
  db: PouchDB.Database,
  field: string,
  time: DateTime | string = now()
): Promise<DatumState> {
  const utcTime =
    typeof time === "string"
      ? parseTimeStr({ timeStr: time }).toUTC().toISO()
      : time.toUTC().toISO();
  if (utcTime === null) {
    throw new Error("bad time");
  }
  let viewResult;
  try {
    viewResult = await db.query(activeStateView.name, {
      startkey: [field, utcTime],
      descending: true,
      limit: 1,
    });
  } catch (error) {
    //TODO make special error checking missingDatumView
    if (
      isCouchDbError(error) &&
      ["missing", "deleted", "missing_named_view"].includes(error.reason)
    ) {
      throw new DatumViewMissingError();
    }
    throw error;
  }

  if (viewResult.rows.length === 0) {
    return null;
  }
  const activeStateRow = viewResult.rows[0];
  if (activeStateRow.key[0] !== field) {
    return null;
  }
  return activeStateRow.value;
}
