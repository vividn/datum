import { CouchDbError } from "./errors";
import { EitherPayload } from "./documentControl/DatumDocument";
import * as connectDbModule from "./auth/connectDb";
import Mock = jest.Mock;
import { DateTime, Settings } from "luxon";
import { parseTimeStr } from "./time/parseTimeStr";
import { now } from "./time/timeUtils";
import PouchDb from "pouchdb";
import { connectDb } from "./auth/connectDb";

// eslint-disable-next-line @typescript-eslint/no-empty-function
export const pass = (): void => {};
export const fail = (): never => {
  throw Error;
};

export const mockDocMissingError: CouchDbError = {
  scope: "couch",
  statusCode: 404,
  errid: "non_200",
  description: "missing",
  error: "not_found",
  reason: "missing",
};

export const mockDocDeletedError: CouchDbError = {
  scope: "couch",
  statusCode: 404,
  errid: "non_200",
  description: "deleted",
  error: "not_found",
  reason: "deleted",
};

export const mockMissingNamedViewError: CouchDbError = {
  scope: "couch",
  statusCode: 404,
  errid: "non_200",
  description: "missing_named_view",
  error: "not_found",
  reason: "missing_named_view",
};

export async function resetTestDb(
  dbName: string
): Promise<PouchDB.Database<EitherPayload>> {
  const db = connectDb({ db: dbName });
  await db.destroy().catch(pass);
  return connectDb({ db: dbName });
}

export function testDbLifecycle(
  dbName: string
): PouchDB.Database<EitherPayload> {
  let db: PouchDB.Database<EitherPayload> = connectDb({ db: dbName });

  beforeEach(async () => {
    db = await resetTestDb(dbName);
    jest.spyOn(connectDbModule, "connectDb").mockImplementation(() => db);
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

// export async function generateSampleDay(dateStr = "2022-08-14") {
//
//   setNow(dateStr);
//   at('8:30', addCmd)({field: "sleep", })
//
//   popNow();
// }
