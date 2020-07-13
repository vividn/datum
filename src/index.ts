#!/usr/bin/env node

import { relTimeStr, combineDateTime } from './time-utils';
import { camelCase, snakeCase } from 'lodash';
import Nano, { MaybeDocument } from 'nano';
import fs from 'fs';
import path from 'path';
import yargs from 'yargs';

// Take a timestamp as soon as possible for accuracy
const currentTime = new Date();

// Separated out from yargs to extract key names later to find extra keys not explicityly included in the options
const yargsOptions = {
  field: {
    describe: 'the primary field of the data',
    alias: 'f',
    nargs: 1,
    type: 'string',
  },
  db: {
    describe: 'The database to use',
    alias: 'database',
    default: 'datum',
  },
  id: {
    describe:
      'Which field(s) to use for the _id field in the document.' +
      ' Can either be a single string with fields delimited by --id-delimiter' +
      ' or can be used multiple times to progressively assemble an id delimited by --id-delimiter',
    alias: ['pk', 'primary', '_id'],
    default: 'time',
  },
  'id-delimiter': {
    describe: 'spacer between fields in the id',
    default: '_',
    type: 'string',
  },
  date: {
    describe:
      'date of the timestamp, use `+n` or `-n` for a date relative to today. If no time is specified with -t, -T is assumed.',
    alias: 'd',
    nargs: 1,
    type: 'string',
    coerce: (arg: string) => {
      if (/^[+-][0-9.]+$/.test(arg)) {
        return relTimeStr(Number(arg), 'days');
      }
      return arg;
    },
  },
  yesterday: {
    describe:
      "use yesterday's date. Equivalent to `-d yesterday`. Use multiple times to go back more days",
    alias: ['y', 'D'],
    type: 'count',
    conflicts: 'date',
    default: undefined,
    coerce: (n?: number) =>
      n === undefined ? undefined : relTimeStr(-n, 'days'),
  },
  time: {
    describe:
      'specify time of the timestamp, use `+n` or `-n` for a timestamp n minutes relative to now',
    alias: 't',
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
  quick: {
    describe:
      'quick options for time, use multiple times. -q = 5 min ago, -qq = 10 min ago, etc.',
    alias: 'q',
    type: 'count',
    conflicts: 'time',
    default: undefined,
    coerce: (n: number) =>
      n === undefined ? undefined : relTimeStr(-5 * n, 'minutes'),
  },
  'full-day': {
    describe:
      'make an entry for the full day, without a specific timestamp, occurs also when -d is used without -t',
    alias: 'f',
    type: 'boolean',
    conflicts: 't',
    coerce: (b: boolean) => (b ? 'today' : undefined), // essentially used to alias `-d today` if no -d flag is specified,
  },
  undo: {
    describe: 'undoes the last datum entry, can be combined with -f',
    alias: 'u',
    type: 'boolean',
  },
  'force-undo': {
    describe:
      'forces an undo, even if the datapoint was entered more than 15 minutes ago',
    alias: 'U',
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
  interactive: {
    describe:
      'Interactive mode. Responds to key presses on the keyboard for rapid data collection',
    alias: 'i',
    conflicts: ['d', 't', 'D', 'T'],
  },
} as const; // as const needed to get yargs typing to work
let argv = yargs
  .command('datum', 'quickly insert timestamped data into couchdb')
  .help('h')
  .alias('h', 'help')

  .options(yargsOptions)
  .example(
    "alias foobar='datum -f abc -K foo bar -k'\nfoobar 3 6",
    'creates a document with the abc field {foo: 3, bar: 6}'
  ).argv;
// console.log(argv);

const auth = JSON.parse(
  fs.readFileSync(path.resolve(__dirname, '../credentials.json')).toString()
);
const nano = Nano(`http://${auth.user}:${auth.pass}@localhost:5984`);
const db = nano.use(argv.db);

type strIndObj = { [index: string]: any };
const getLongOptionData = function(
  argv: strIndObj,
  options: strIndObj
): strIndObj {
  const args = { ...argv };
  // delete any keys that are explicitly options in yargs
  Object.entries(options).forEach(entry => {
    const [key, value] = entry;

    delete args[key];
    delete args[camelCase(key)];
    delete args[snakeCase(key)];

    const aliases: string[] = [].concat(value ?? []);
    for (const alias of aliases) {
      delete args[alias];
      delete args[camelCase(alias)];
      delete args[snakeCase(alias)];
    }
  });
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
  const positionals: (string | number)[] = argv._ ?? [];
  const [withKey, withoutKey] = positionals.reduce(
    (result, element) => {
      if (typeof element === 'string' && element.includes('=')) {
        result[0].push(element);
      } else {
        result[1].push(element);
      }
      return result;
    },
    [[] as string[], [] as (string | number)[]]
  );

  for (const arg of withKey) {
    const [key, value] = arg.split('=');
    payload[key] = Number(value) || value;
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

    const positionalValue = withoutKey.shift();
    if (defaultValue === undefined) {
      if (noMoreRequiredPositionals) {
        throw 'All required extra keys must come before all optional keys';
      }
      if (positionalValue === undefined) {
        throw `No data given for the required key '${dataKey}`;
      }
    }
    if (defaultValue !== undefined) {
      noMoreRequiredPositionals = true;
    }
    // default value is '' when nothing is given after the =
    payload[dataKey] =
      positionalValue ??
      (defaultValue === '' ? undefined : Number(defaultValue) || defaultValue);
  }
  if (withoutKey.length > 0) {
    throw 'some data do not have keys. Either use long options `--key value`, equals signs `key=value`, assign predefined keys in the alias `-K key1 key2 -k value1 value2`, or use -A to pull an array into a single key `-A key value1 value2 value3 -a';
  }

  return payload;
};

const longOptionData = getLongOptionData(argv, yargsOptions);
const payload = parsePositional(argv, longOptionData);

const argDate =
  argv.date ??
  (argv.yesterday as string | undefined) ??
  (argv['full-day'] as string | undefined);
const argTime: string | undefined =
  argv.time ?? (argv.quick as string | undefined);

const timings = {
  time: combineDateTime(argDate, argTime, currentTime),
  creationTime: currentTime.toISOString(),
};

const dataDocument = argv.field
  ? { ...timings, [argv.field]: payload }
  : { ...timings, ...payload };

const calculateId = function(
  argId: string | string[],
  delimiter: string,
  data: strIndObj
): string {
  function fieldNameToValue(name: string) {
    if (!(name in data)) {
      throw 'Data required by _id is not present';
    }
    return String(data[name]);
  }

  if (typeof argId === 'string') {
    return argId
      .split(delimiter)
      .map(fieldNameToValue)
      .join(delimiter);
  }
  // string[]
  return argId.map(fieldNameToValue).join(delimiter);
};
const _id = calculateId(argv.id, argv['id-delimiter'], dataDocument); // TODO: Make this flexible

db.insert(dataDocument as MaybeDocument, _id)
  .then(() => db.get(_id))
  .then((body: any) => console.log(body))
  .catch((err: any) => console.log(err));

// TODO: parse JSON arguments as Objects
// TODO: parse array strings as arrays
// TODO: handle conflicts
