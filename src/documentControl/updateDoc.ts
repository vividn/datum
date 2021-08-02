import { DocumentScope } from "nano";
import { EitherPayload } from "./DatumDocument";
import { GenericObject } from "../GenericObject";

type updateDocType = {
  db: DocumentScope<EitherPayload>;
  id: string;
  payload: EitherPayload;
  updateStrategy: UpdateStrategyNames;
};

type UpdateStrategyNames =
  | "useOld"
  | "useNew"
  | "preferOld"
  | "preferNew"
  | "intersection"
  | "removeConflicting"
  | "xor"
  | "merge";

const updateStrategies: Record<UpdateStrategyNames, CombiningType> = {
  useOld: { justA: true, justB: false, same: true, conflict: "A" },
  useNew: { justA: false, justB: true, same: true, conflict: "B" },
  preferOld: { justA: true, justB: true, same: true, conflict: "A" },
  preferNew: { justA: true, justB: true, same: true, conflict: "B" },
  intersection: { justA: false, justB: false, same: true, conflict: false },
  removeConflicting: { justA: true, justB: true, same: true, conflict: false },
  xor: { justA: true, justB: true, same: false, conflict: false },
  merge: { justA: true, justB: true, same: true, conflict: "merge" },
};

type CombiningType = {
  justA: boolean;
  justB: boolean;
  same: boolean;
  conflict: "A" | "B" | "merge" | false;
};

// const combineData = (aData: GenericObject, bData: GenericObject)
