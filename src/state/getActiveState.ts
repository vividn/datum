import { DateTime } from "luxon";
import { DatumState } from "./activeStateView";

export function getActiveState(db: PouchDB.Database, field: string, time?: DateTime = DateTime.local()): Promise<DatumState> {}