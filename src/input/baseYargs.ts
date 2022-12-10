import yargs from "yargs";
import { OutputArgs, outputYargs, Show } from "./outputArgs";

export type BaseDatumArgs = {
  db?: string;
  host?: string;
  username?: string;
  password?: string;
  env?: string;
  createDb?: boolean;
  _?: string[];
} & OutputArgs;

export const baseYargs = outputYargs(yargs
  .options({
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
  })
  .commandDir("../commands")
  .help("h")
  .alias("h", "help")
  .strict());
// TODO: Middleware to enforce proper non array types
