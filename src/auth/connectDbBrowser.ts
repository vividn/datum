import PouchDb from "pouchdb";
import idbAdapter from "pouchdb-adapter-idb";
import { EitherPayload } from "../documentControl/DatumDocument";
import { MainDatumArgs } from "../input/mainArgs";

PouchDb.plugin(idbAdapter);

export function connectDbBrowser(
  args: MainDatumArgs,
): PouchDB.Database<EitherPayload> {
  const { db: dbName = "datum" } = args;

  return new PouchDb(dbName, { adapter: "idb" });
}
