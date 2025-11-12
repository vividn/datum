import { EitherPayload } from "../documentControl/DatumDocument.js";
import { MainDatumArgs } from "../input/mainArgs.js";
import { mergeConfigAndEnvIntoArgs } from "../config/mergeConfigIntoArgs.js";
import { connectDbBrowser } from "./connectDbBrowser.js";
import { connectDbMemory } from "./connectDbMemory.js";
import { connectDbFile } from "./connectDbFile.js";
import { connectDbHttp } from "./connectDbHttp.js";

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
