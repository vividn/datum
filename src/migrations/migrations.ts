import { UpdateStrategyNames } from "../documentControl/combineData";
import { EitherDocument } from "../documentControl/DatumDocument";
import { GenericObject } from "../GenericObject";
import { DatumView } from "../views/DatumView";
import { _emit } from "../views/emit";

export function getMigrationViewName(shortName: string): string {
  return `migrate__${shortName}`;
}
export function getMigrationId(shortName: string): string {
  return `_design/${getMigrationViewName(shortName)}`;
}

export type MigrationOps = UpdateStrategyNames | "overwrite" | "delete";
const MigrationOps: Record<MigrationOps, any> = {
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
  overwrite: "",
  delete: "",
};
export const migrationOps = Object.keys(MigrationOps);

type MigrationMapKey = any; // user defined sort for orderin migration operations
type MigrationMapValue = {
  op: MigrationOps;
  value: GenericObject;
};

export function migrationEmit(
  key: MigrationMapValue,
  value: MigrationMapValue,
) {
  _emit(key, value);
}

export type DatumMigration = DatumView<
  EitherDocument,
  MigrationMapKey,
  MigrationMapValue,
  undefined
> & {
  name: `migrate_${string}`;
  reduce: undefined;
  namedReduce: undefined;
};
