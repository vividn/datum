import yargs from "yargs";
import { outputYargs } from "./outputArgs";

export type BaseArgs = {
  db?: string;
  host?: string;
  adapter?: string;
  username?: string;
  password?: string;
  env?: string;
  createDb?: boolean;
};

export const baseArgs = outputYargs(
  yargs.options({
    // couchdb options
    db: {
      describe: "The database to use, defaults to datum",
      alias: "database",
      nargs: 1,
    },
    host: {
      describe: "Host and port to use, defaults to 'localhost:5984'",
      nargs: 1,
    },
    adapter: {
      describe: "PouchDb adapter to use, will default to PouchDBs choice",
      nargs: 1,
    },
    username: {
      describe: "couchdb username to use'",
      nargs: 1,
    },
    password: {
      describe: "couchdb password to use'",
      nargs: 1,
    },
    env: {
      describe:
        "Environment file to read with COUCHDB_USER, COUCHDB_PASSWORD, COUCHDB_HOST",
      nargs: 1,
      normalize: true,
    },
    "create-db": {
      describe: "Create the db if it does not exist",
      type: "boolean",
    },
  }),
).strict();

// TODO: Middleware to enforce proper non array types
