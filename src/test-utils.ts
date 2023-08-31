import { CouchDbError } from "./errors";
import {
  DataOnlyDocument,
  DatumData,
  DatumDocument,
  DatumMetadata,
  EitherDocument,
  EitherPayload,
} from "./documentControl/DatumDocument";
import Mock = jest.Mock;
import { DateTime, Settings } from "luxon";
import { parseTimeStr } from "./time/parseTimeStr";
import { now } from "./time/timeUtils";
import { connectDb } from "./auth/connectDb";
import * as connectDbModule from "./auth/connectDb";
import { defaultIdComponents } from "./ids/defaultIdComponents";
import { buildIdStructure } from "./ids/buildIdStructure";
import { defaults } from "./input/defaults";
import { assembleId } from "./ids/assembleId";

// eslint-disable-next-line @typescript-eslint/no-empty-function
export const pass = (): void => {};
export const fail = (): never => {
  throw Error;
};

export const mockDocMissingError: CouchDbError = {
  status: 404,
  name: "not_found",
  error: "not_found",
  reason: "missing",
  message: "missing",
};

export const mockDocDeletedError: CouchDbError = {
  status: 404,
  message: "deleted",
  name: "not_found",
  error: "not_found",
  reason: "deleted",
};

export const mockMissingNamedViewError: CouchDbError = {
  status: 404,
  name: "not_found",
  error: "not_found",
  reason: "missing_named_view",
  message: "missing_named_view",
};

export async function resetTestDb(
  db: PouchDB.Database & {
    __opts?: PouchDB.Configuration.DatabaseConfiguration;
    _destroyed?: boolean;
    _closed?: boolean;
  }
): Promise<PouchDB.Database> {
  await db.destroy().catch(pass);
  // nasty hack to reopen closed database
  delete db._destroyed;
  delete db._closed;
  return db.constructor(db.name, db.__opts);
}

export function testDbLifecycle(
  dbName: string
): PouchDB.Database<EitherPayload> {
  const db = connectDb({ db: dbName });

  beforeEach(async () => {
    await resetTestDb(db);
    jest.spyOn(connectDbModule, "connectDb").mockReturnValue(db);
  });

  afterEach(async () => {
    await db.destroy().catch(pass);
  });

  return db;
}

export function mockedLogLifecycle(): Mock {
  const originalLog = console.log;
  const mockedLog = jest.fn() as Mock;

  beforeEach(async () => {
    console.log = mockedLog;
  });

  afterEach(async () => {
    console.log = originalLog;
    mockedLog.mockReset();
  });

  return mockedLog;
}

const originalNowFn = Settings.now;
const nowStack: (() => number)[] = [];
export function setNow(timeStr: string): DateTime {
  const parsedTime = parseTimeStr({ timeStr });
  const newNow = () => parsedTime.toMillis();
  Settings.now = newNow;
  return parsedTime;
}

export function pushNow(timeStr: string): DateTime {
  nowStack.push(Settings.now);
  return setNow(timeStr);
}

export function popNow(): DateTime {
  const lastNow = nowStack.pop();
  if (lastNow === undefined) {
    throw new Error("tried to pop non existent now");
  }
  Settings.now = lastNow;
  return now();
}

export function restoreNow(): DateTime {
  Settings.now = originalNowFn;
  nowStack.length = 0;
  return now();
}

export function at<A extends any[], O>(
  timeStr: string,
  fn: (...args: A) => O
): (...args: A) => O {
  return (...args: A): O => {
    pushNow(timeStr);
    const returnVal: O = fn(...args);
    popNow();
    return returnVal;
  };
}

// TODO: Transition all tests to use makeDoc where appropriate
export function makeDoc(
  data: DatumData,
  meta: DatumMetadata | false = {},
  include_rev = false
): EitherDocument {
  let doc: EitherDocument;
  if (meta === false) {
    doc = { ...data } as DataOnlyDocument;
  } else {
    doc = { ...data, meta } as DatumDocument;
  }

  meta = meta || {};
  if (doc._id === undefined) {
    let idStructure: string;
    if (meta?.idStructure) {
      idStructure = meta.idStructure;
    } else {
      const { defaultIdParts, defaultPartitionParts } = defaultIdComponents({
        data,
      });
      idStructure = buildIdStructure({
        idParts: defaultIdParts,
        delimiter: defaults.idDelimiter,
        partition: defaultPartitionParts,
      });
    }
    doc._id = assembleId({ payload: doc, idStructure });
  }

  if (include_rev) {
    doc._rev = "1-foo";
  }
  return doc;
}

// export async function generateSampleDay(dateStr = "2022-08-14") {
//
//   setNow(dateStr);
//   at('8:30', addCmd)({field: "sleep", })
//
//   popNow();
// }
