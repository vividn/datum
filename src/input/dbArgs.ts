import { ArgumentParser, BooleanOptionalAction } from "argparse";

export type DbArgs = {
  db?: string;
  host?: string;
  adapter?: string;
  user?: string;
  password?: string | null;
  env?: string;
  createDb?: boolean;
  configFile?: string;
};

const parser = new ArgumentParser({
  add_help: false,
});

const dbGroup = parser.add_argument_group({
  title: "Database Options",
  description: "Options for connecting to a CouchDb or PouchDb database",
});
dbGroup.add_argument("--db", "--database", {
  help: "The database to use, defaults to datum",
});
dbGroup.add_argument("--host", {
  help: "Host and port to use, defaults to 'localhost:5984'",
});
dbGroup.add_argument("--adapter", {
  help: "PouchDb adapter to use, will default to PouchDBs choice",
});
dbGroup.add_argument("--username", {
  help: "couchdb username to use",
});
dbGroup.add_argument("--password", {
  help: "couchdb password to use",
});
dbGroup.add_argument("--env", {
  help: "Environment file to read with COUCHDB_USER, COUCHDB_PASSWORD, COUCHDB_HOST",
});
dbGroup.add_argument("--config", {
  help: "datum configuration file to use rather than the default at ~/.config/datum/datumrc.yml",
  dest: "configFile",
});
dbGroup.add_argument("--create-db", {
  help: "Create the db if it does not exist. Note: this only has an effect on couchdb instances, not local pouchdb",
  action: BooleanOptionalAction,
  dest: "createDb",
});

export const dbArgs = parser;
