import yargs from "yargs";

export type DatumYargsType = {
  db?: string;
  host?: string;
  username?: string;
  password?: string;
  env?: string;
  payload?: string;
  date?: string;
  yesterday?: number;
  time?: string;
  quick?: number;
  timezone?: string;
  fullDay?: boolean;
  noTimestamp?: boolean;
  noMetadata?: boolean;
  field?: string;
  comment?: string | string[];
  idPart?: string | string[];
  idDelimiter?: string;
  partition?: string;
  undo?: boolean;
  required?: string | string[];
  optional?: string | string[];
  remainder?: string;
  stringRemainder?: boolean;
  lenient?: boolean;
  _?: (string | number)[];
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

    payload: {
      describe:
        "Base payload to add keys to, or used for raw document input into couchdb. Use with --no-metadata for unmodified entry. Default: {}",
      nargs: 1,
      alias: "p",
      type: "string",
    },

    // timing
    date: {
      describe:
        "date of the timestamp, use `+n` or `-n` for a date relative to today. If no time is specified with -t, -T is assumed.",
      alias: "d",
      nargs: 1,
      type: "string",
    },
    yesterday: {
      describe:
        "use yesterday's date. Equivalent to `-d yesterday`. Use multiple times to go back more days",
      alias: "y",
      type: "count",
    },
    time: {
      describe:
        "specify time of the timestamp, use `+n` or `-n` for a timestamp n minutes relative to now",
      alias: "t",
      nargs: 1,
      type: "string",
    },
    quick: {
      describe:
        "quick options for time, use multiple times. -q = 5 min ago, -qq = 10 min ago, etc.",
      alias: "q",
      type: "count",
    },
    timezone: {
      describe:
        "Set the timezone to use instead of local time. Accepts both timezone names (America/Chicago) and utc offsets '-7'",
      alias: "z",
      type: "string",
    },
    "full-day": {
      describe:
        "make an entry for the full day, without a specific timestamp, occurs also when -d is used without -t",
      alias: "D",
      type: "boolean",
    },
    "no-timestamp": {
      describe: "omit the occurTime from the data",
      alias: "T",
      type: "boolean",
    },
    "no-metadata": {
      describe: "do not include meta data in document",
      alias: "M",
      type: "boolean",
    },
    // data
    field: {
      describe:
        "field specifying what is being tracked, used by default as partition for the data, but can be changed with --partition",
      alias: "f",
      nargs: 1,
      type: "string",
    },
    comment: {
      describe: "comment to include in the data",
      alias: "c",
      nargs: 1,
      type: "string",
    },

    // id
    "id-part": {
      describe:
        "Which field(s) to use for the _id field in the document." +
        " Can either be a single string with fields delimited by --id-delimiter" +
        " or can be used multiple times to progressively assemble an id delimited by --id-delimiter",
      alias: ["id", "pk", "_id"],
      type: "string",
    },
    "id-delimiter": {
      describe: "spacer between fields in the id",
      type: "string",
    },
    partition: {
      describe:
        "field to use for the partition (default: field, specified with -f)." +
        " Can be fields of data or raw strings surrounded by single quotes." +
        " Like --id-field, can be used  mulitple times to assemble a partition separated by --id-delimiter",
      type: "string",
    },
    undo: {
      describe: "undoes the last datum entry, can be combined with -f",
      alias: "u",
      type: "boolean",
    },
    // "force-undo": {
    //   describe:
    //     "forces an undo, even if the datapoint was entered more than 15 minutes ago",
    //   alias: "U",
    //   type: "boolean",
    // },
    required: {
      describe:
        "Add a required key to the data, will be filled with first keyless data. If not enough data is specified to fill all required keys, an error will be thrown",
      alias: ["K", "req"],
      type: "string",
      nargs: 1,
    },
    optional: {
      describe:
        "Add an optional key to the data, will be filled with first keyless data. A default value can be specified with an '=', e.g., -k key=value",
      alias: ["k", "opt"],
      type: "string",
      nargs: 1,
    },
    remainder: {
      describe:
        "Any extra data supplied will be put into this key as an array. When --lenient is specified, defaults to 'extraData'",
      alias: ["rem", "R"],
      type: "string",
      narags: 1,
    },
    "string-remainder": {
      describe:
        "Remainder data will be a space concatenated string rather than an array",
      alias: "S",
      type: "boolean",
    },
    lenient: {
      describe: "Allow extra data without defined keys",
      type: "boolean",
      alias: "l",
    },
  })
  .help("h")
  .alias("h", "help");
