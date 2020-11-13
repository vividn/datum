const yargs = require("yargs");

const configuredYargs = yargs
  .command("datum", "quickly insert timestamped data into couchdb")
  .options({
    // couchdb options
    db: {
      describe: "The database to use",
      alias: "database",
      default: "datum",
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
    "full-day": {
      describe:
        "make an entry for the full day, without a specific timestamp, occurs also when -d is used without -t",
      alias: "D",
      type: "boolean",
    },
    "no-timestamp": {
      describe: "omit the occurTime from the data",
      alias: "T",
      type: "boolean"
    },
    "no-metadata": {
      describe: "do not include meta data in document",
      alias: "M",
      type: "boolean"
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
    "id-field": {
      describe:
        "Which field(s) to use for the _id field in the document." +
        " Can either be a single string with fields delimited by --id-delimiter" +
        " or can be used multiple times to progressively assemble an id delimited by --id-delimiter",
      alias: ["id", "pk", "_id"],
      type: "string",
      default: "meta.occurTime",
    },
    "id-delimiter": {
      describe: "spacer between fields in the id",
      default: "__",
      type: "string",
    },
    partition: {
      describe:
        "field to use for the partition (default: field, specified with -f)." +
        " Can be fields of data or raw strings surrounded by single quotes." +
        " Like --id-field, can be used  mulitple times to assemble a partition separated by --id-delimiter",
      type: "string",
      default: "field",
    },
    // undo: {
    //   describe: "undoes the last datum entry, can be combined with -f",
    //   alias: "u",
    //   type: "boolean",
    // },
    // "force-undo": {
    //   describe:
    //     "forces an undo, even if the datapoint was entered more than 15 minutes ago",
    //   alias: "U",
    //   type: "boolean",
    // },
    extraKeys: {
      describe:
        "The keys to use for additional data, useful for aliases. Use multiple times for multiple keys. " +
        "Keys can be used with equals signs for optionality or default values. " +
        "`datum -k KEY1 -k KEY2= -k KEY3=default` can then take 1-3 positional args, with KEY3 being set to default if < 3 are given",
      alias: "k",
      type: "string",
      nargs: 1,
    },
    lenient: {
      describe: "Allow extra data without defined keys",
      type: "boolean",
      alias: "l",
    },
  })
  .help("h")
  .alias("h", "help")
  .example(
    "alias foobar='datum -f abc -K foo bar -k'\nfoobar 3 6",
    "creates a document with the abc field {foo: 3, bar: 6}"
  );

module.exports = { configuredYargs };
