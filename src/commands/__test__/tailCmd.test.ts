import { pass, resetTestDb, testNano } from "../../test-utils";
import { DocumentScope } from "nano";
import { EitherPayload } from "../../documentControl/DatumDocument";
import { afterAll, afterEach, beforeEach, jest } from "@jest/globals";
import * as connectDb from "../../auth/connectDb";

const originalLog = console.log;

describe("tailCmd", () => {
  const mockedLog = jest.fn();
  const dbName = "tail_cmd_test";
  const db = testNano.use(dbName) as DocumentScope<EitherPayload>;
  const connectDbSpy = jest
    .spyOn(connectDb, "default")
    .mockImplementation(() => db);

  beforeEach(async () => {
    await resetTestDb(dbName);
    console.log = mockedLog;
  });

  afterEach(async () => {
    await testNano.db.destroy(dbName).catch(pass);
    console.log = originalLog;
    mockedLog.mockReset();
  });

  afterAll(async () => {
    connectDbSpy.mockRestore();
  });

  it.todo("displays the last 10 occurences in the database");
  it.todo("can display the last n occurences");
})