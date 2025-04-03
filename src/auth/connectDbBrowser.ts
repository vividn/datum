import { EitherPayload } from "../documentControl/DatumDocument";
import { MainDatumArgs } from "../input/mainArgs";

export function connectDbBrowser(
  args: MainDatumArgs,
): PouchDB.Database<EitherPayload> {
  const { db: dbName = "datum" } = args;

  return new PouchDB(dbName, { adapter: "indexeddb" });
}
