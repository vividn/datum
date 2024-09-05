import { DateTime } from "luxon";
import { stateChangeView } from "../views/datumViews/stateChangeView";
import { DatumViewMissingError, isCouchDbError } from "../errors";
import { parseTimeStr } from "../time/parseTimeStr";
import { DatumState } from "./normalizeState";
import { DatumTime, isDatumTime } from "../time/datumTime";
import { now } from "../time/timeUtils";

export async function getActiveState(
  db: PouchDB.Database,
  field: string,
  time: DateTime | DatumTime | string = now(),
): Promise<DatumState> {
  const utcTime =
    typeof time === "string"
      ? parseTimeStr({ timeStr: time }).toUTC().toISO()
      : isDatumTime(time)
        ? time.utc
        : time.toUTC().toISO();
  if (utcTime === null) {
    throw new Error("bad time");
  }
  let viewResult;
  try {
    viewResult = await db.query(stateChangeView.name, {
      startkey: [field, utcTime],
      descending: true,
      limit: 1,
      reduce: false,
    });
  } catch (error) {
    //TODO make special error checking missingDatumView
    if (
      isCouchDbError(error) &&
      ["missing", "deleted", "missing_named_view"].includes(error.reason)
    ) {
      throw new DatumViewMissingError(stateChangeView.name);
    }
    throw error;
  }

  if (viewResult.rows.length === 0) {
    return null;
  }
  const activeStateRow = viewResult.rows[0];
  if (activeStateRow.key[0] !== field) {
    // the case where there are no states for the field so the view returns the preivous field with limit: 1
    return null;
  }
  return activeStateRow.value[1];
}
