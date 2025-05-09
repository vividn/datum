import PouchDb from "pouchdb";
import { EitherPayload } from "../documentControl/DatumDocument";
import { MainDatumArgs } from "../input/mainArgs";

export function connectDbFile(
  args: MainDatumArgs,
): PouchDB.Database<EitherPayload> {
  const { db: dbName = "datum", createDb } = args;

  let host = args.host;
  const adapter =
    process.env["POUCHDB_ADAPTER"] || host === "%MEMORY%"
      ? "memory"
      : undefined;

  if (host === undefined || host === "") {
    const dataDir =
      process.env["XDG_DATA_HOME"] ?? process.env["HOME"] + "/.local/share";
    host = `${dataDir}/datum`;
  }

  const fullDatabaseName = !host
    ? dbName
    : host.at(-1) === "/"
      ? `${host}${dbName}`
      : `${host}/${dbName}`;

  if (host.startsWith("/")) {
    import("fs").then((fs) => {
      // create parent directories
      if (!fs.existsSync(fullDatabaseName)) {
        fs.mkdirSync(fullDatabaseName, { recursive: true });
      }
    })
  }

  const couchAuth = {
    username: args.user,
    password: args.password,
  };
  return new PouchDb(fullDatabaseName, {
    skip_setup: !createDb,
    auth: couchAuth,
    adapter,
  });
}
