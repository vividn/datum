import { ArgumentParser, BooleanOptionalAction } from "argparse";

export type DbArgs = {
  db?: string;
  host?: string;
  adapter?: string;
  username?: string;
  password?: string;
  env?: string;
  createDb?: boolean;
};

const parser = new ArgumentParser({
  add_help: false,
});

const dbGroup = parser.add_argument_group({
  title: "Database Options",
  description: "Options for connecting to a CouchDb or PouchDb database",
});
parser.add_argument("--db", "--database", {
  help: "The database to use, defaults to datum",
  nargs: 1,
});
dbGroup.add_argument("--host", {
  help: "Host and port to use, defaults to 'localhost:5984'",
  nargs: 1,
});
dbGroup.add_argument("--adapter", {
  help: "PouchDb adapter to use, will default to PouchDBs choice",
  nargs: 1,
});
dbGroup.add_argument("--username", {
  help: "couchdb username to use",
  nargs: 1,
});
dbGroup.add_argument("--password", {
  help: "couchdb password to use",
  nargs: 1,
});
dbGroup.add_argument("--env", {
  help: "Environment file to read with COUCHDB_USER, COUCHDB_PASSWORD, COUCHDB_HOST",
  nargs: 1,
});
dbGroup.add_argument("--create-db", {
  help: "Create the db if it does not exist",
  type: BooleanOptionalAction,
});

export const dbArgs = parser;
