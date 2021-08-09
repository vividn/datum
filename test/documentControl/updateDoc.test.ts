import {
  DatumPayload,
  EitherPayload,
} from "../../src/documentControl/DatumDocument";
import { DateTime, Settings } from "luxon";
import {
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  test,
} from "@jest/globals";
import { pass, testNano } from "../test-utils";
import timezone_mock from "timezone-mock";
import {
  combineData,
  conflictStrategies,
} from "../../src/documentControl/combineData";

const testDatumPayload: DatumPayload = {
  data: {
    abc: 123,
    foo: "bar",
  },
  meta: {
    occurTime: "2021-06-20T14:00:00Z",
    utcOffset: 2,
    random: 0.4869350234,
    idStructure: "%foo%__rawString",
    humanId: "ndke4ms9",
  },
};

const testDatumPayloadId = "bar__rawString";
const mockNow = DateTime.utc(2021, 6, 20, 18, 45, 0);
const now = mockNow.toString();
const notNow = DateTime.utc(2010, 11, 12, 13, 14, 15).toString();

describe("updateDoc", () => {
  const dbName = "update_doc_test";
  const db = testNano.db.use<EitherPayload>(dbName);

  beforeAll(async () => {
    await testNano.db.destroy(dbName).catch(pass);
  });

  beforeEach(async () => {
    await testNano.db.create(dbName).catch(pass);
  });

  afterEach(async () => {
    await testNano.db.destroy(dbName).catch(pass);
  });

  test("it returns the updated document in the db", async () => {
    const oldDoc1 = { _id: "docId1", data: { abc: "123" }, meta: {} };
    const returnedDoc1 = await updateDoc({
      db,
      id: "docId1",
      payload: {
        newKey: "newData",
      },
    });
    const dbDoc1 = await db.get("docId1");
    expect(returnedDoc1).toEqual(dbDoc1);

    const oldDoc2 = { _id: "docId2", def: "456" };
    const returnedDoc2 = await updateDoc("docId2", { newKey: "newData" });
    const dbDoc2 = await db.get("docId2");
    expect(returnedDoc2).toEqual(dbDoc2);
  });

  test("updates modifyTime if oldDoc has metadata", async () => {
    timezone_mock.register("UTC");
    Settings.now = () => mockNow.toMillis();

    const docWithModify = {
      _id: "doc-to-update",
      data: { abc: "def" },
      meta: { modifyTime: notNow },
    };
    await db.insert(docWithModify);

    const newDoc = await updateDoc({
      db,
      id: "doc-to-update",
      payload: {
        newKey: "newData",
      },
    });

    expect(newDoc).toHaveProperty("meta.modifyTime", now);

    timezone_mock.unregister();
    Settings.resetCaches();
  });

  test("does not add metadata if oldDoc does not have metadata", () => {
    await db.insert({ _id: "docWithoutMeta", key: "data" });
    const newDoc = await updateDoc({
      db,
      id: "docWithoutMeta",
      payload: testDatumPayload,
    });
    expect(newDoc).not.toHaveProperty("meta");
  });

  test.todo(
    "keeps all the metadata in oldDoc (except modifyTime), and does not add anything from the payload"
  );

  test.todo(
    "each combination of dataOnly and datum for oldDoc and payload call the appropriate combine function on just the data component"
  );

  test("if the new document has a different calculated or explicit id, then the doc is moved there", async () => {
    await db.insert({ _id: "old-id", foo: "bar" });
    const newDoc = await updateDoc({
      db,
      id: "old-id",
      payload: { _id: "new-id", other: "data" },
      updateStrategy: "preferNew",
    });
    await expect(db.get("old-id")).rejects.toThrow("deleted");
    expect(newDoc).toMatchObject({ _id: "new-id", foo: "bar", other: "data" });
  });

  test.todo("it does not modify the input payload");
});
