import yargs from "yargs";
import { Show } from "../output/output";

export type BaseDatumArgs = {
  db?: string;
  host?: string;
  username?: string;
  password?: string;
  env?: string;
  showAll?: boolean;
  show?: Show;
  createDb?: boolean;
  _?: string[];
};

export const baseYargs = yargs
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
    "show-all": {
      describe: "Show complete document when displaying, not just data",
      type: "boolean",
      alias: "A",
    },
    show: {
      describe: "how much of documents to show",
      type: "string",
      choices: Object.values(Show),
      default: "default",
      conflict: "show-all",
    },
    "format-string": {
      describe: "create a custom output string for visualizing the doc(s). Specify %keys% with percent signs",
      type: "string",
      alias: "o"
    },
    "create-db": {
      describe: "Create the db if it does not exist",
      type: "boolean",
    },
  })
  .commandDir("../commands")
  .help("h")
  .alias("h", "help")
  .strict();
// TODO: Middleware to enforce proper non array types
