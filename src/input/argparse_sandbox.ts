import { ArgumentParser } from "argparse";
import * as shellQuote from "shell-quote";
import { MainDatumArgs } from "./mainArgs";

export function argparse_sandbox(cliInput: string | string[]): MainDatumArgs {
  const input =
    typeof cliInput === "string"
      ? shellQuote.parse(cliInput).map(String)
      : cliInput;
  const parser = new ArgumentParser({
    description: "argparse sandbox",
  });

  parser.add_argument("--db", {
    help: "The database to use, defaults to datum",
    dest: "db",
  });
  parser.add_argument("--host", {
    help: "Host and port to use, defaults to 'localhost:5984'",
    dest: "host",
  });
  parser.add_argument("--adapter", {
    help: "PouchDb adapter to use, will default to PouchDBs choice",
    dest: "adapter",
  });
  parser.add_argument("--username", {
    help: "couchdb username to use",
    dest: "username",
  });
  parser.add_argument("--password", {
    help: "couchdb password to use",
    dest: "password",
  });
  parser.add_argument("--env", {
    help: "Environment file to read with COUCHDB_USER, COUCHDB_PASSWORD, COUCHDB_HOST",
    dest: "env",
  });
  parser.add_argument("--create-db", {
    help: "Create the db if it does not exist",
    action: "store_true",
    dest: "createDb",
  });

  return parser.parse_args(input);
}
