import Nano, { DocumentScope } from "nano";
import { CouchDbError } from "./errors";
import { EitherPayload } from "./documentControl/DatumDocument";
import * as connectDb from "./auth/connectDb";
import Mock = jest.Mock;
import { DateTime, Settings } from "luxon";
import { parseTimeStr } from "./time/parseTimeStr";
import { now } from "./time/timeUtils";

// eslint-disable-next-line @typescript-eslint/no-empty-function
export const pass = (): void => {};
export const fail = (): never => {
  throw Error;
};

export const testNano = Nano(`http://admin:password@localhost:5983`);

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
): Promise<DocumentScope<EitherPayload>> {
  const maxTries = 3;
  let tries = 0;
  await testNano.db.destroy(dbName).catch(pass);
  while (tries++ <= maxTries) {
    await testNano.db.destroy(dbName).catch(pass);
    await testNano.db.create(dbName).catch(pass);
    if ((await testNano.db.list()).includes(dbName)) {
      return testNano.use(dbName) as DocumentScope<EitherPayload>;
    }
  }
  throw Error(`Unable to reset database after ${maxTries} attempts`);
}

export function testDbLifecycle(dbName: string): DocumentScope<EitherPayload> {
  const db = testNano.use(dbName) as DocumentScope<EitherPayload>;

  beforeEach(async () => {
    await resetTestDb(dbName);
    jest.spyOn(connectDb, "connectDb").mockImplementation(async () => db);
  });

  afterEach(async () => {
    await testNano.db.destroy(dbName).catch(pass);
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
