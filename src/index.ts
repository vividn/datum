#!/usr/bin/env node

// Take a timestamp as soon as possible for accuracy
const currentTime = new Date();

// Separated out from yargs to extract key names later to find extra keys not explicityly included in the options
const yargsOptions = {
  f: {
    describe: 'the primary field of the data',
    alias: 'field',
    nargs: 1,
  },
  db: {
    describe: 'The database to use',
    alias: 'database',
    default: 'datum',
  },
  d: {
    describe:
      'date of the timestamp, use `+n` or `-n` for a date relative to today. If no time is specified with -t, -T is assumed.',
    alias: 'date',
    nargs: 1,
    type: 'string',
  },
  D: {
    describe: "use yesterday's date. Equivalent to `-d yesterday`",
    alias: 'yesterday',
    type: 'boolean',
    conflicts: 'date',
    coerce: () => 'yesterday',
  },
  t: {
    describe:
      'specify time of the timestamp, use `+n` or `-n` for a timestamp n minutes relative to now',
    alias: 'time',
    nargs: 1,
    type: 'string',
  },
  T: {
    describe: 'make an entry for the full day, without a specific timestamp',
    alias: 'full-day',
    type: 'boolean',
    conflicts: 't',
  },
  u: {
    describe: 'undoes the last datum entry, can be combined with -f',
    alias: 'undo',
    type: 'boolean',
  },
  U: {
    describe:
      'forces an undo, even if the datapoint was entered more than 15 minutes ago',
    alias: 'force-undo',
    type: 'boolean',
  },
  K: {
    describe:
      'The keys to use for additional data, useful for aliases. Keys can be used with equals signs for optionality or default values.' +
      '`datum -K KEY1 KEY2= KEY3=default -k` can then take 1-3 positional args, with KEY3 being set to default if <3 are given',
    alias: 'keys',
    type: 'array',
  },
  k: {
    describe: 'Terminate the -K array',
    type: 'boolean',
  },
  A: {
    describe: 'Enter in array data for a key. `-A KEY DATA1 DATA2 ... -a`',
    alias: 'array',
    type: 'array',
  },
  a: {
    describe: 'Terminate the -A array',
    type: 'boolean',
  },
  i: {
    describe:
      'Interactive mode. Responds to key presses on the keyboard for rapid data collection',
    alias: 'interactive',
    conflicts: ['d', 't', 'D', 'T'],
  },
};
let argv = require('yargs')
  .command('datum', 'quickly insert timestamped data into couchdb')
  .help('h')
  .alias('h', 'help')

  .options(yargsOptions)
  .example(
    "alias foobar='datum -f abc -K foo bar -k'\nfoobar 3 6",
    'creates a document with the abc field {foo: 3, bar: 6}'
  ).argv;

const chrono = require('chrono-node');
const fs = require('fs');

const auth = JSON.parse(fs.readFileSync('credentials.json'));
const nano = require('nano')(`http://${auth.user}:${auth.pass}@localhost:5984`);
const db = nano.use(argv.db);

// const getExtraOptions = function(argv, options) {
//   for (const option in options) {
//   }
// };
// const buildDataPayload;

// const parseDataTime = function(dateStr: string, timeStr: string) {
//   return chrono.parseDate(dateStr);
// };

console.log(argv);

const creationTime = currentTime.toISOString();
const datumTime = creationTime;

const dataDocument = { creationTime, time: datumTime };
console.log(dataDocument);
db.insert(dataDocument, creationTime).then((body: any) => console.log(body));

// const eventDate = date ?? (yesterday && 'yesterday');
// const isFullDay = fullDay ?? (date && time === undefined);

// const creationTime = currentTime;
// const eventTime = isFullDay;
//
// const payload = {};
