#!/usr/bin/env node

// Take a timestamp as soon as possible for accuracy
const currentTime = new Date();

const fs = require('fs');
const auth = JSON.parse(fs.readFileSync('credentials.json'));

const nano = require('nano')(`http://${auth.user}:${auth.pass}@localhost:5984`);
const db = nano.use('datum');

const chrono = require('chrono-node');

let argv = require('yargs')
  .command('datum', 'quickly insert timestamped data into couchdb')
  .help('h')
  .alias('h', 'help')

  .options({
    f: {
      describe: 'the primary field of the data',
      alias: 'field',
      nargs: 1,
      demandOption: true,
    },
    d: {
      describe:
        'specify date of the timestamp, use `+n` or `-n` for a date relative to today. If no time is specified with -t, -T is assumed.',
      alias: 'date',
      nargs: 1,
      type: 'string',
    },
    D: {
      describe: "use yesterday's date. Equivalent to `-d yesterday`",
      alias: 'yesterday',
      type: 'boolean',
      conflicts: 'date',
      coerce: () => parseArgDate('yesterday'),
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
    A: {
      describe: 'The keys to use for additional data, useful for aliases',
      alias: 'additional-data',
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
  })
  .example(
    "alias foobar='datum -f abc -A foo bar -a'; foobar 3 6",
    'creates a document with the abc field {foo: 3, bar: 6}'
  ).argv;

const parseArgDate = function(dateStr: string) {
  return chrono.parseDate(dateStr);
};

console.log(argv);

const creationTime = currentTime.toISOString();
const dataTime = creationTime;

const dataDocument = { creationTime, time: dataTime };

db.insert(dataDocument).then((body: any) => console.log(body));

// const eventDate = date ?? (yesterday && 'yesterday');
// const isFullDay = fullDay ?? (date && time === undefined);

// const creationTime = currentTime;
// const eventTime = isFullDay;
//
// const payload = {};
