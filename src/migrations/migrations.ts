import { DatumData, DatumMetadata, EitherDocument } from "../documentControl/DatumDocument";

export function getMigrationViewName(shortName: string): string {
  return `migrate__${shortName}`;
}
export function getMigrationId(shortName: string): string {
  return `_design/${getMigrationViewName(shortName)}`;
}

export function migrationEmit(priority: number, { op: }) {}

export type DatumMigration = {
  name: string;
  map: (doc: EitherDocument) => void;
}
