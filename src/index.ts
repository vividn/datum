import { combineDateTime } from './time-utils';
import Nano, { MaybeDocument } from 'nano';
import fs from 'fs';
import path from 'path';
import { strIndObj } from './utils';
import { buildPayloadFromInput, configuredYargs } from './input';

// Take a timestamp as soon as possible for accuracy
const currentTime = new Date();

const argv = configuredYargs.argv;
const payload = buildPayloadFromInput(argv);

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

const auth = JSON.parse(
  fs.readFileSync(path.resolve(__dirname, '../credentials.json')).toString()
);
const nano = Nano(`http://${auth.user}:${auth.pass}@couchdb:5984`);
const db = nano.use(argv.db);

db.insert(dataDocument as MaybeDocument, _id)
  .then(() => db.get(_id))
  .then((body: any) => console.log(body))
  .catch((err: any) => console.log(err));

// TODO: handle conflicts
