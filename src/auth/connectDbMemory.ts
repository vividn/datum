import PouchDb from "pouchdb-core";
import memoryAdapter from "pouchdb-adapter-memory";
import { EitherPayload } from "../documentControl/DatumDocument";
import { MainDatumArgs } from "../input/mainArgs";

PouchDb.plugin(memoryAdapter);

export function connectDbMemory(
  args: MainDatumArgs,
): PouchDB.Database<EitherPayload> {
  const { db: dbName = "datum" } = args;
  return new PouchDb(dbName, {
    adapter: "memory",
  });
}
