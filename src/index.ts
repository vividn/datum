#!/usr/bin/env node

const { relTimeStr } = require('./time-utils');

// Take a timestamp as soon as possible for accuracy
// const currentTime = new Date();

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
    coerce: (arg: string) => {
      if (/^[+-][0-9.]+$/.test(arg)) {
        return relTimeStr(Number(arg), 'days');
      }
      return arg;
    },
  },
  y: {
    describe:
      "use yesterday's date. Equivalent to `-d yesterday`. Use multiple times to go back more days",
    alias: ['yesterday', 'D'],
    type: 'count',
    conflicts: 'date',
    default: undefined,
    coerce: (n?: number) =>
      n === undefined ? undefined : relTimeStr(-n, 'days'),
  },
  t: {
    describe:
      'specify time of the timestamp, use `+n` or `-n` for a timestamp n minutes relative to now',
    alias: 'time',
    nargs: 1,
    type: 'string',
    coerce: (arg: string) => {
      if (/^[+-][0-9.]+$/.test(arg)) {
        return relTimeStr(Number(arg), 'minutes');
      }
      if (/^[0-9]{3,4}$/.test(arg)) {
        // quick shortcut to turn just numbers into times. ex. 330->3:30, 1745->17:45
        return arg.slice(0, -2) + ':' + arg.slice(-2);
      }
      return arg;
    },
  },
  q: {
    describe:
      'quick options for time, use multiple times. -q = 5 min ago, -qq = 10 min ago, etc.',
    alias: 'quick',
    type: 'count',
    conflicts: 'time',
    default: undefined,
    coerce: (n: number) =>
      n === undefined ? undefined : relTimeStr(-5 * n, 'minutes'),
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

console.log(argv);

// const chrono = require('chrono-node');
// const fs = require('fs');
//
// const auth = JSON.parse(fs.readFileSync('credentials.json'));
// const nano = require('nano')(`http://${auth.user}:${auth.pass}@localhost:5984`);
// const db = nano.use(argv.db);
//
// const getExtraOptions: object = function(argv: object, options: object) {
//   for (const option in options) {
//     delete argv[option];
//     let aliases = [].concat(options[option].alias ?? [])
//     if argv[option][alias]
//   }
// };
// const buildDataPayload;

//
// // Must use toString here because the coerce in yargs is run before count turns the args back into numbers
// const argDate: string | undefined = argv.date ?? argv.yesterday?.toString();
// const argTime: string | undefined = argv.time ?? argv.quick?.toString();
//
// const parseDateStr: string = function(dateStr?: string) {
//   if (dateStr === undefined) {
//     return undefined;
//   }
//
//   if (/[+-][0-9]+/.matchAll(dateStr)) {
//     const relativeDays = Number(dateStr);
//     const fullDateTime =
//       relativeDays > 0
//         ? chrono.parseDate(`${relativeDays} from now`)
//         : chrono.parseDate(`${relativeDays} ago`);
//   }
// };
//
// const parseDataTime = function(
//   dateStr?: string,
//   timeStr?: string,
//   isFullDay?: boolean
// ) {
//   const onlyDay = isFullDay || (dateStr !== undefined && timeStr === undefined);
//   if (dateStr === undefined && timeStr === undefined) {
//     return onlyDay
//       ? chrono
//           .parseDate('today')
//           .toISOString()
//           .split('T')[0]
//       : currentTime.toISOString();
//   }
//   if (dateStr !== undefined) {
//   }
//   // Just a placeholder for now
//   return chrono.parseDate(dateStr || timeStr);
// };
//
// const creationTime = currentTime.toISOString();
// const datumTime = parseDataTime;
//
// const dataDocument = { creationTime, time: datumTime };
// console.log(dataDocument);
// db.insert(dataDocument, creationTime).then((body: any) => console.log(body));

// const eventDate = date ?? (yesterday && 'yesterday');
// const isFullDay = fullDay ?? (date && time === undefined);

// const creationTime = currentTime;
// const eventTime = isFullDay;
//
// const payload = {};
