import { DatumData } from "../documentControl/DatumDocument";
import set from "lodash.set";
import { DataArgs } from "../input/dataArgs";
import { normalizeState } from "../state/normalizeState";
import get from "lodash.get";
import unset from "lodash.unset";
import { toDatumTime } from "../time/timeUtils";
import { DateTime } from "luxon";

// TODO: Use alterDatumData to change everything here rather than set/unset
export const commandChanges = ["occur", "start", "end", "switch"] as const;
export type CommandChange = (typeof commandChanges)[number];
// WARNING: Changes datumData and args in place
export function changeDatumCommand(
  datumData: DatumData,
  command: CommandChange,
  args?: DataArgs,
) {
  switch (command) {
    case "occur":
      datumData.occurTime ??= toDatumTime(DateTime.local());
      datumData.dur = null;
      break;

    case "start":
      datumData.occurTime ??= toDatumTime(DateTime.local());
      const isActive = !!(
        datumData.state &&
        normalizeState(datumData.state) &&
        get(datumData, "state.id") !== false
      );
      if (!isActive) {
        set(datumData, "state.id", true);
      }
      if (datumData.dur === null) {
        unset(datumData, "dur");
      }
      if (Array.isArray(args?.optional)) {
        args!.optional.unshift("dur");
      }
      break;

    case "end":
      datumData.occurTime ??= toDatumTime(DateTime.local());
      if (
        datumData.state === undefined ||
        normalizeState(datumData.state) !== false
      ) {
        if (
          datumData.state === undefined ||
          normalizeState(datumData.state) !== true
        ) {
          datumData.lastState ??= datumData.state;
        }
        datumData.state = { id: false };
      }
      if (datumData.dur === null) {
        unset(datumData, "dur");
      }
      if (Array.isArray(args?.optional)) {
        args!.optional.unshift("dur");
      }
      break;

    case "switch":
      datumData.occurTime ??= toDatumTime(DateTime.local());
      set(datumData, "state", {});
      if (datumData.dur === null) {
        unset(datumData, "dur");
      }
      if (Array.isArray(args?.optional)) {
        args!.optional.unshift("dur");
        args!.optional.unshift("state.id=true");
      }
      break;
  }
}
