import { DatumData } from "../documentControl/DatumDocument";
import { jClone } from "../utils/jClone";
import { normalizeState } from "./normalizeState";
import { getActiveState } from "./getActiveState";

export async function compileState(
  db: PouchDB.Database,
  payloadData: DatumData,
): Promise<DatumData> {
  const data = jClone(payloadData);
  if (data.state !== undefined) {
    data.state = normalizeState(data.state);
  }
  if (data.lastState !== undefined) {
    data.lastState = normalizeState(data.lastState);
    return data;
  }
  if (data.field !== undefined && data.occurTime !== undefined) {
    const lastState = await getActiveState(db, data.field, data.occurTime);
    if (lastState === null || data.state !== undefined) {
      data.lastState = lastState;
    }
  }
  return data;
}
