import { ArgumentParser, BooleanOptionalAction } from "argparse";
import { OutputArgs, outputYargs } from "./outputArgs";

export type BaseArgs = {
  db?: string;
  host?: string;
  adapter?: string;
  username?: string;
  password?: string;
  env?: string;
  createDb?: boolean;
  _?: (string | number)[];
} & OutputArgs;

export function dbArgs(parser: ArgumentParser): ArgumentParser {
  parser.add_argument("--db", "--database", {
    help: "The database to use, defaults to datum",
    nargs: 1,
  });
  parser.add_argument("--host", {
    help: "Host and port to use, defaults to 'localhost:5984'",
    nargs: 1,
  });
  parser.add_argument("--adapter", {
    help: "PouchDb adapter to use, will default to PouchDBs choice",
    nargs: 1,
  });
  parser.add_argument("--username", {
    help: "couchdb username to use",
    nargs: 1,
  });
  parser.add_argument("--password", {
    help: "couchdb password to use",
    nargs: 1,
  });
  parser.add_argument("--env", {
    help: "Environment file to read with COUCHDB_USER, COUCHDB_PASSWORD, COUCHDB_HOST",
    nargs: 1,
  });
  parser.add_argument("--create-db", {
    help: "Create the db if it does not exist",
    type: BooleanOptionalAction,
  });
  return parser;
}
