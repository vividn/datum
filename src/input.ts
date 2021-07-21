import yargs from "yargs";

export type BaseDatumArgs = {
  db?: string;
  host?: string;
  username?: string;
  password?: string;
  env?: string;
  showAll?: boolean;
  _?: string[];
  autoCreateDb?: boolean;
};

export const configuredYargs = yargs
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
    "auto-create-db": {
      describe: "Create db if it does not exist",
      type: "boolean",
      hidden: true,
    },
  })
  .commandDir("commands")
  .command("hello [dataa..]", "hello world message")
  .help("h")
  .alias("h", "help")
  .strict();
// TODO: Middleware to enforce proper non array types
