import { UpdateStrategyNames } from "../documentControl/combineData.js";
import { EitherDocument } from "../documentControl/DatumDocument.js";
import { JsonObject, JsonType } from "../utils/utilityTypes.js";
import { DatumView, MapRow } from "../views/DatumView.js";
import { _emit } from "../views/emit.js";

type MigrationName = `migrate_${string}`;
export function migrationName(shortName: string): MigrationName {
  return `migrate_${shortName}`;
}
export function getMigrationId(shortName: string): string {
  return `_design/${migrationName(shortName)}`;
}

export type MigrationOps = UpdateStrategyNames | "overwrite" | "delete";
const MigrationOps: Record<MigrationOps, ""> = {
  merge: "",
  useOld: "",
  preferOld: "",
  update: "",
  preferNew: "",
  useNew: "",
  removeConflicting: "",
  xor: "",
  intersection: "",
  append: "",
  prepend: "",
  appendSort: "",
  mergeSort: "",
  rekey: "",
  overwrite: "",
  delete: "",
} as const;
export const migrationOps = Object.keys(MigrationOps);

type MigrationMapKey = JsonType; // user defined sort for orderin migration operations
export type MigrationMapValue = {
  op: MigrationOps;
  data: JsonObject;
};

export function migrationEmit(key: MigrationMapKey, value: MigrationMapValue) {
  _emit(key, value);
}

export type DatumMigration = DatumView<
  EitherDocument,
  MigrationMapKey,
  MigrationMapValue
> & {
  name: MigrationName;
  reduce: "_count";
};

export type MigrationMapRow = MapRow<DatumMigration>;
