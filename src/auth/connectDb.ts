import { EitherPayload } from "../documentControl/DatumDocument";
import { MainDatumArgs } from "../input/mainArgs";
import { mergeConfigAndEnvIntoArgs } from "../config/mergeConfigIntoArgs";
import { connectDbBrowser } from "./connectDbBrowser";
import { connectDbMemory } from "./connectDbMemory";
import { connectDbFile } from "./connectDbFile";

export function connectDb(
  args: MainDatumArgs,
): PouchDB.Database<EitherPayload> {
  mergeConfigAndEnvIntoArgs(args);

  try {
  if (window !== undefined) {
    return connectDbBrowser(args);
  }
  } catch (e) {
    // pass
  }

  const host = args.host;
  const adapter =
    process.env["POUCHDB_ADAPTER"] || host === "%MEMORY%"
      ? "memory"
      : undefined;
  if (adapter === "memory") {
    return connectDbMemory(args);
  }
  return connectDbFile(args);
}
