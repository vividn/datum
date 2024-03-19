import { JsonType } from "./utilityTypes";
import get from "lodash.get";
import { createOrAppend } from "./createOrAppend";
import set from "lodash.set";
import unset from "lodash.unset";
import { DatumData } from "../documentControl/DatumDocument";
import { inferType } from "./inferType";
import { parseTimeStr } from "../time/parseTimeStr";
import {
  isoDateFromDateTime,
  isoDurationFromDuration,
  toDatumTime,
} from "../time/timeUtils";
import { parseDateStr } from "../time/parseDateStr";
import { parseDurationStr } from "../time/parseDurationStr";

// TODO: Always keep state keys normalized
// TODO: write function to easily add new fields to a normalized state without data loss
export function alterDatumData({
  datumData,
  path,
  value,
  defaultValue,
  append,
}: {
  datumData: DatumData;
  path: string;
  value: JsonType | undefined;
  defaultValue?: JsonType;
  append?: boolean;
}): void {
  const stateAwarePath =
    path === "state"
      ? "state.id"
      : path.startsWith(".")
        ? `state${path}`
        : path;

  let inferredValue: JsonType | undefined;
  switch (true) {
    case value === ".": {
      return alterDatumData({
        datumData,
        path,
        value: defaultValue,
        append,
      });
    }

    case value === undefined:
    case value === "":
    case /^undefined$/i.test(String(value)): {
      inferredValue = undefined;
      break;
    }

    case /^null$/i.test(String(value)): {
      inferredValue = null;
      break;
    }

    case /(?:\b|_)time\d*$/i.test(stateAwarePath):
    case /[a-z0-9]Time\d*$/.test(stateAwarePath): {
      const parsedTime = parseTimeStr({ timeStr: String(value) });
      inferredValue = toDatumTime(parsedTime);
      break;
    }

    case /(?:\b|_)date\d*$/i.test(stateAwarePath):
    case /[a-z0-9]Date\d*$/.test(stateAwarePath): {
      const parsedDate = parseDateStr({ dateStr: String(value) });
      inferredValue = isoDateFromDateTime(parsedDate);
      break;
    }

    case /(?:\b|_)dur(ation)?\d*$/i.test(stateAwarePath):
    case /[a-z0-9]Dur(ation)?\d*$/.test(stateAwarePath): {
      const parsedDuration = parseDurationStr({ durationStr: String(value) });
      inferredValue = isoDurationFromDuration(parsedDuration);
      break;
    }

    case typeof value === "string": {
      inferredValue = inferType(value as string);
      break;
    }

    default: {
      inferredValue = value;
      break;
    }
  }

  if (append) {
    if (inferredValue === undefined) {
      return;
    }
    const current = get(datumData, stateAwarePath);
    const newValue = createOrAppend(current, inferredValue);
    set(datumData, stateAwarePath, newValue);
  } else {
    set(datumData, stateAwarePath, inferredValue);
  }
}
