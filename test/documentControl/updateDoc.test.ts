import {
  DatumDocument,
  DatumMetadata,
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
  jest,
  test,
} from "@jest/globals";
import { pass, testNano } from "../test-utils";
import timezone_mock from "timezone-mock";
import updateDoc, {
  NoDocToUpdateError,
  UpdateDocError,
} from "../../src/documentControl/updateDoc";
import addDoc from "../../src/documentControl/addDoc";
import * as combineData from "../../src/documentControl/combineData";
import jClone from "../../src/utils/jClone";

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
    await db.insert({ _id: "docId1", data: { abc: "123" }, meta: {} });
    const returnedDoc1 = await updateDoc({
      db,
      id: "docId1",
      payload: {
        newKey: "newData",
      },
    });
    const dbDoc1 = await db.get("docId1");
    expect(returnedDoc1).toEqual(dbDoc1);

    await db.insert({ _id: "docId2", def: "456" });
    const returnedDoc2 = await updateDoc({
      db,
      id: "docId2",
      payload: { newKey: "newData" },
    });
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

  test("does not add metadata if oldDoc does not have metadata", async () => {
    await db.insert({ _id: "docWithoutMeta", key: "data" });
    const newDoc = await updateDoc({
      db,
      id: "docWithoutMeta",
      payload: testDatumPayload,
    });
    expect(newDoc).not.toHaveProperty("meta");
  });

  test("keeps all the metadata in oldDoc (except modifyTime), and does not add anything from the payload", async () => {
    const oldDoc = (await addDoc({
      db,
      payload: testDatumPayload,
    })) as DatumDocument;
    const newPayload = {
      data: { newKey: "newData" },
      meta: {
        occurTime: "2021-08-09T14:13:00Z",
        utcOffset: 1,
        idStructure: "%newKey%",
        random: 0.666,
        humanId: "noneOfThisShouldBeIncluded",
      },
    } as DatumPayload;
    const newDoc = (await updateDoc({
      db,
      id: testDatumPayloadId,
      payload: newPayload,
    })) as DatumDocument;

    const oldDocMeta = { ...oldDoc.meta };
    delete oldDocMeta.modifyTime;
    expect(newDoc.meta).toMatchObject(oldDocMeta);

    await expect(db.get("newData")).rejects.toThrow("missing");
  });

  test("different combination of dataOnly and datum for oldDoc and payload call the combineData function with appropriate arguments data component", async () => {
    const data1 = { abc: 123 };
    const data2 = { def: 456 };
    const data3 = { two: "fields", andTwo: "data" };
    const data4 = { another: null, set: "ofData" };
    const metadata = { ...testDatumPayload.meta } as DatumMetadata;
    delete metadata.idStructure;

    const spy = jest.spyOn(combineData, "default");

    await db.insert({ _id: "data-doc-1", ...data1 });
    await updateDoc({ db, id: "data-doc-1", payload: data2 });
    expect(spy).toHaveBeenCalledWith(
      { _id: "data-doc-1", ...data1 },
      data2,
      "merge"
    );
    spy.mockClear();

    await db.insert({ _id: "data-doc-2", ...data3 });
    const datumPayload2 = { data: data4, meta: metadata } as DatumPayload;
    await updateDoc({
      db,
      id: "data-doc-2",
      payload: datumPayload2,
      updateStrategy: "removeConflicting",
    });
    expect(spy).toHaveBeenCalledWith(
      { _id: "data-doc-2", ...data3 },
      data4,
      "removeConflicting"
    );
    spy.mockClear();

    await db.insert({ _id: "datum-doc-3", data: data2, meta: metadata });
    await updateDoc({
      db,
      id: "datum-doc-3",
      payload: data3,
      updateStrategy: "intersection",
    });
    expect(spy).toHaveBeenCalledWith(data2, data3, "intersection");
    spy.mockClear();

    await db.insert({ _id: "datum-doc-4", data: data4, meta: metadata });
    await updateDoc({
      db,
      id: "datum-doc-4",
      payload: { data: data1, meta: metadata },
      updateStrategy: "append",
    });
    expect(spy).toHaveBeenCalledWith(data4, data1, "append");
    spy.mockClear();

    spy.mockRestore();
  });

  test("fails if id to be updated does not exist in db", async () => {
    await expect(
      updateDoc({ db, id: "does-not-exist", payload: { valid: "data" } })
    ).rejects.toThrowError(NoDocToUpdateError);
  });

  test("fails if new id clashes with a different document in the database", async () => {
    const oldId = "id-to-replace";
    const clashingId = "preexisting-clashing-id";
    await db.insert({ _id: oldId });
    await db.insert({ _id: clashingId });

    await expect(
      updateDoc({
        db,
        id: oldId,
        payload: { _id: clashingId, foo: "bar" },
        updateStrategy: "useNew",
      })
    ).rejects.toThrowError(UpdateDocError);
    await db.get(oldId); // original doc is not deleted

    const oldCalculatedId = "old-calc-id";
    await db.insert({
      _id: oldCalculatedId,
      data: { foo: oldCalculatedId },
      meta: { idStructure: "%foo%" },
    });
    await expect(
      updateDoc({
        db,
        id: oldCalculatedId,
        payload: {
          foo: clashingId,
        },
        updateStrategy: "useNew",
      })
    ).rejects.toThrowError(UpdateDocError);
    await db.get(oldCalculatedId);
  });

  test("if updated dataonly payload does not have id, then it uses the old id", async () => {
    await db.insert({ _id: "data-id", dataKey: "abc" });
    const newDoc = await updateDoc({
      db,
      id: "data-id",
      payload: { newKey: "newData" },
      updateStrategy: "useNew",
    });
    expect(newDoc._id).toEqual("data-id");
    expect(newDoc).not.toHaveProperty("dataKey");
  });

  test("if the new document has a different explicit id, then the doc is moved there", async () => {
    await db.insert({ _id: "old-id", foo: "bar" });
    const newDoc = await updateDoc({
      db,
      id: "old-id",
      payload: { _id: "new-id", other: "data" },
      updateStrategy: "preferNew",
    });
    await expect(db.get("old-id")).rejects.toThrow("deleted");
    expect(await db.get("new-id")).toEqual(newDoc);
    expect(newDoc).toMatchObject({ _id: "new-id", foo: "bar", other: "data" });
  });

  test("if the new document has a different calculated id, then the doc is moved there", async () => {
    await db.insert({
      _id: "calculated-id",
      data: { foo: "calculated-id" },
      meta: { idStructure: "%foo%" },
    });
    const newDoc = await updateDoc({
      db,
      id: "calculated-id",
      payload: { foo: "new-calculated-id" },
      updateStrategy: "preferNew",
    });
    await expect(db.get("calculated-id")).rejects.toThrow("deleted");
    expect(newDoc._id).toEqual("new-calculated-id");
    expect(await db.get("new-calculated-id")).toEqual(newDoc);
    expect(newDoc).toHaveProperty("data.foo", "new-calculated-id");
  });

  test("if payload specified a _rev, then it must match the _rev on the old document", async () => {
    await db.insert({ _id: "abc", foo: "bar" });
    const oldDoc = await db.get("abc");
    const wrongRev = "1-38748349796ad6a60a11c0f63d10186a";

    await expect(
      updateDoc({
        db,
        id: "abc",
        payload: { _id: "abc", _rev: wrongRev, foo2: "bar2" },
        updateStrategy: "useNew",
      })
    ).rejects.toThrowError(UpdateDocError);
    const stillOldDoc = await db.get("abc");
    expect(oldDoc).toEqual(stillOldDoc);

    await updateDoc({
      db,
      id: "abc",
      payload: { _id: "abc", _rev: oldDoc._rev, foo3: "bar3" },
      updateStrategy: "useNew",
    });

    const newDoc = await db.get("abc");
    expect(newDoc).toHaveProperty("foo3", "bar3");

    await expect(
      updateDoc({
        db,
        id: "abc",
        payload: { _rev: wrongRev, ...testDatumPayload },
        updateStrategy: "useNew",
      })
    ).rejects.toThrowError(UpdateDocError);
    await updateDoc({
      db,
      id: "abc",
      payload: { _rev: newDoc._rev, ...testDatumPayload },
      updateStrategy: "useNew",
    });
  });

  test("it does not modify the input payload", async () => {
    await db.insert({ _id: "ididid", abc: 123 });
    const payload = { key1: "data1", key2: "data2" };
    const payloadClone = jClone(payload);

    await updateDoc({ db, id: "ididid", payload: payload });
    expect(payload).toEqual(payloadClone);
  });

  test("it successfully merges new data into the existing data", async () => {
    await db.insert({
      _id: "doc-id",
      oldKey: "oldData",
      mutualKey: ["merge", "basis"],
      anotherMutual: "fromOld",
    });
    const newDoc = await updateDoc({
      db,
      id: "doc-id",
      payload: {
        newKey: "newData",
        mutualKey: ["will", "merge", "without", "duplicates"],
        anotherMutual: null,
      },
      updateStrategy: "merge",
    });
    expect(newDoc).toMatchObject({
      _id: "doc-id",
      oldKey: "oldData",
      newKey: "newData",
      mutualKey: ["merge", "basis", "will", "without", "duplicates"],
      anotherMutual: ["fromOld", null],
    });
  });

  test.todo("how does it handle _ids for dataonly docs?");

  test("it does not write to database if updated data document is identical", async () => {
    await db.insert({ _id: "datadoc-id", foo: "abc" });
    const currentDoc = await db.get("datadoc-id");
    const nDocsBefore = (await db.info()).doc_count;

    const newDoc = await updateDoc({
      db,
      id: "datadoc-id",
      payload: { foo: "abc" },
      updateStrategy: "useNew",
    });
    expect(newDoc._rev).toEqual(currentDoc._rev);
    const nDocsAfter = (await db.info()).doc_count;
    expect(nDocsBefore).toEqual(nDocsAfter);
  });

  test("it does not write to db if updated datum document is identical", async () => {
    // Datum
    await db.insert({
      _id: "datum-id",
      data: { abc: "foo" },
      meta: { humanId: "datumDatum" },
    });
    const currentDDoc = await db.get("datum-id");
    const nDocsBefore2 = (await db.info()).doc_count;

    const newDDoc = await updateDoc({
      db,
      id: "datum-id",
      payload: {
        data: { keys: "that", will: "be ignored" },
        meta: { random: 0.010101 },
      },
      updateStrategy: "useOld",
    });
    expect(newDDoc._rev).toEqual(currentDDoc._rev);
    const nDocsAfter2 = (await db.info()).doc_count;
    expect(nDocsBefore2).toEqual(nDocsAfter2);
  });
});
