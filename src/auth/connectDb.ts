import { EitherPayload } from "../documentControl/DatumDocument";
import { MainDatumArgs } from "../input/mainArgs";
import { mergeConfigAndEnvIntoArgs } from "../config/mergeConfigIntoArgs";
import { connectDbBrowser } from "./connectDbBrowser";
import { connectDbMemory } from "./connectDbMemory";
import { connectDbFile } from "./connectDbFile";
import { connectDbHttp } from "./connectDbHttp";

export function connectDb(
  args: MainDatumArgs,
): PouchDB.Database<EitherPayload> {
  mergeConfigAndEnvIntoArgs(args);

  try {
    if (window !== undefined) {
      return connectDbBrowser(args);
    }
  } catch {
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
  if (host?.startsWith("http://") || host?.startsWith("https://")) {
    return connectDbHttp(args);
  }
  return connectDbFile(args);
}
