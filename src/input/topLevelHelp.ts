import { ArgumentParser, RawDescriptionHelpFormatter } from "argparse";

export const topLevelHelpParser = new ArgumentParser({
  prog: "datum",
  description: `
Track data on your life.

Usage:
  datum <command> [options]

Commands:
  add         Add a new document to the database.
  occur       Record the occurrence of an event at a specific time.
  start       Record the start of an event that occurs in blocks of time.
  end, stop   Record the end of an event.
  switch      Switch the state of a given field.
  get         Retrieve and display a document by its ID.
  update      Update an existing document with new data.
  merge       Merge new data into an existing document.
  rekey       Change the key of an existing document.
  delete, del Delete a document from the database.
  map         Display a map view or map-reduce view of the data.
  reduce, red Perform a reduction on a map view.
  setup       Set up the database for use with datum.
  tail        Show the most recent entries in the database.
  head        Show the earliest entries in the database.
  edit        Edit a document directly using your editor.
  v1          Export data in the old datum format.
  grep        Search for documents matching a pattern.
  backup      Create a backup of the database.
  restore     Restore the database from a backup file.
  migrate     Migrate data from one state to another.
  check       Check for problems in the data and optionally fix them.
  sync        Synchronize the database with a remote host.
  retime      Change the time of an existing document.
  dayview     Generate a visual representation of data for a day.
  nowview     Generate a visual representation of data of the current state and recent history
  
Options:
  --db, --database <name>    Specify the database to use (default: "datum").
  --host <host>              Specify the host and port (default: "localhost:5984").
  --username <user>          Specify the username for authentication.
  --password <password>      Specify the password for authentication.
  --env <file>               Load environment variables from a file.
  --config <file>            Use a custom configuration file.
  --create-db                Create the database if it does not exist.
  --show-all, -A             Show complete documents in output.
  --show <level>             Specify the level of detail in output (e.g., "minimal", "standard").
  --format-string, -o <fmt>  Use a custom format string for output.`,
  epilog: `Run 'datum <command> --help' for more information about a specific command.`,
  add_help: false,
  formatter_class: RawDescriptionHelpFormatter,
});
