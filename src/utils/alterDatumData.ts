import { JsonType } from "./utilityTypes";
import get from "lodash.get";
import { createOrAppend } from "./createOrAppend";
import set from "lodash.set";
import { DatumData } from "../documentControl/DatumDocument";

export function alterDatumData({
  datumData,
  path,
  value,
  defaultValue,
  append,
}: {
  datumData: DatumData;
  path: string;
  value: JsonType;
  defaultValue?: JsonType;
  append?: boolean;
}): void {
  const stateAwarePath =
    path === "state"
      ? "state.id"
      : path.startsWith(".")
        ? `state${path}`
        : path;

  if (append) {
    const current = get(datumData, stateAwarePath);
    const newValue = createOrAppend(current, value);
    set(datumData, stateAwarePath, newValue);
  } else {
    set(datumData, stateAwarePath, value);
  }
}
