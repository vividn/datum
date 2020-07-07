#!/usr/bin/env node

const { relTimeStr, combineDateTime } = require('./time-utils');
const _ = require('lodash');

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
  F: {
    describe:
      'make an entry for the full day, without a specific timestamp, occurs also when -d is used without -t',
    alias: 'full-day',
    type: 'boolean',
    conflicts: 't',
    coerce: (b: boolean) => (b ? 'today' : undefined), // essentially used to alias `-d today` if no -d flag is specified,
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
    alias: 'extra-keys',
    type: 'array',
    default: [],
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

const fs = require('fs');

const auth = JSON.parse(fs.readFileSync('credentials.json'));
const nano = require('nano')(`http://${auth.user}:${auth.pass}@localhost:5984`);
const db = nano.use(argv.db);

type strIndObj = { [index: string]: any };
const getLongOptionData = function(
  argv: strIndObj,
  options: strIndObj
): strIndObj {
  const args = { ...argv };
  // delete any keys that are explicitly options in yargs
  for (const option in options) {
    delete args[option];
    const aliases: string[] = [].concat(options[option].alias ?? []);
    for (const alias of aliases) {
      delete args[alias];
      delete args[_.camelCase(alias)];
      delete args[_.snakeCase(alias)];
    }
  }
  // And the built in ones
  delete args['_'];
  delete args['$0'];

  return args;
};

const parsePositional = function(
  argv: strIndObj,
  currentPayload?: strIndObj
): strIndObj {
  const payload: strIndObj = currentPayload ?? {};
  const positionals: string[] = argv._ ?? [];
  const [withKey, withoutKey] = positionals.reduce(
    (result, element) => {
      result[element.includes('=') ? 1 : 0].push(element);
      return result;
    },
    [[] as string[], [] as string[]]
  );

  for (const arg of withKey) {
    const [key, value] = arg.split('=');
    payload[key] = value;
  }

  let noMoreRequiredPositionals = false;
  for (const extraKey of argv.extraKeys) {
    const [dataKey, defaultValue, tooManyEquals] = extraKey.split('=');
    if (tooManyEquals !== undefined) {
      throw 'Too many equals signs in a key in --extra-keys';
    }

    // the data key might be manually specified
    if (dataKey in payload) {
      continue;
    }

    const positionalValue = withoutKey.pop();
    if (defaultValue === undefined) {
      if (noMoreRequiredPositionals) {
        throw 'All required extra keys must come before all optional keys';
      }
      if (positionalValue === undefined) {
        throw `No data given for the required key '${dataKey}`;
      }
    }
    // default value is '' when nothing is given after the =
    if (defaultValue !== undefined) {
      noMoreRequiredPositionals = true;
    }
    payload[dataKey] = positionalValue ?? (defaultValue || undefined);
  }

  return payload;
};

const longOptionData = getLongOptionData(argv, yargsOptions);
const payload = parsePositional(argv, longOptionData);

const argDate: string | undefined = argv.date ?? argv.yesterday ?? argv.fullDay;
const argTime: string | undefined = argv.time ?? argv.quick;

const timings = {
  datumTime: combineDateTime(argDate, argTime, currentTime),
  creationTime: currentTime.toISOString(),
};

const dataDocument = argv.field
  ? { ...timings, [argv.field]: payload }
  : { ...timings, ...payload };

const primaryKey = dataDocument.datumTime; // TODO: Make this flexible
console.log(primaryKey);
db.insert(dataDocument, primaryKey)
  .then(() => db.get(primaryKey))
  .then((body: any) => console.log(body))
  .catch((err: any) => console.log(err));
