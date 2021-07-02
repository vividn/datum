import {
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
} from "@jest/globals";

import { fail, pass, testNano } from "./test-utils";
import addDoc from "../src/documentControl/addDoc";
import {
  DataOnlyDocument,
  DataOnlyPayload,
  DatumDocument,
  DatumPayload,
  EitherPayload,
} from "../src/documentControl/DatumDocument";
import timezone_mock from "timezone-mock";
import { DateTime, Settings } from "luxon";
import { IdError } from "../src/errors";
import overwriteDoc from "../src/documentControl/overwriteDoc";

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

describe("addDoc", () => {
  const dbName = "add_doc_test";
  const db = testNano.db.use<EitherPayload>(dbName);

  beforeAll(async () => {
    await testNano.db.destroy(dbName).catch(pass);
  });

  beforeEach(async () => {
    await testNano.db.create(dbName).catch(pass);
    timezone_mock.register("UTC");
    Settings.now = () => mockNow.toMillis();
  });

  afterEach(async () => {
    await testNano.db.destroy(dbName).catch(pass);
    timezone_mock.unregister();
    Settings.resetCaches();
  });

  it("it adds dataOnly payloads with _id to the given database", async () => {
    const id = "dataOnlyPayloadId";
    const payload = { _id: id, a: 1, c: "dataString" };

    const returnedDoc = await addDoc({ db, payload });
    const dbDoc = (await db.get(id)) as DataOnlyDocument;
    expect(returnedDoc).toEqual(dbDoc);
    expect(dbDoc).toMatchObject(payload);
  });

  it("throws error if trying to add dataOnly payload without _id", async () => {
    const payload = { a: 1, c: "dataString" };
    await expect(addDoc({ db, payload })).rejects.toThrowError(IdError);
  });

  it("adds a datum payload to db with _id if there is no idStructure", async () => {
    const payload = {
      _id: "DatumData_with_no_structure",
      data: { abc: "abc" },
      meta: { humanId: "abc" },
    } as DatumPayload;

    const returnedDoc = await addDoc({ db, payload });
    const dbDoc = (await db.get(
      "DatumData_with_no_structure"
    )) as DatumDocument;
    expect(returnedDoc).toEqual(dbDoc);
    expect(dbDoc).toMatchObject(payload);
  });

  it("adds a datum payload to the database with its calculated id", async () => {
    const payload = testDatumPayload;
    const returnedDoc = await addDoc({ db, payload });
    const dbDoc = (await db.get(testDatumPayloadId)) as DatumDocument;

    expect(returnedDoc).toEqual(dbDoc);
    expect(dbDoc).toMatchObject(testDatumPayload);
  });

  it("prefers to use the id calculated from structure", async () => {
    const payload = testDatumPayload;

    const returnedDoc = await addDoc({ db, payload });
    const dbDoc = (await db.get(testDatumPayloadId)) as DatumDocument;
    expect(returnedDoc).toEqual(dbDoc);
    expect(dbDoc).toMatchObject(testDatumPayload);
  });

  it("throws if datumPayload has neither id nor idStructure", async () => {
    const payload = {
      data: { abc: "123" },
      meta: { humanId: "ndke4ms9" },
    } as DatumPayload;
    await expect(addDoc({ db, payload })).rejects.toThrowError(IdError);
  });

  it("adds createTime and modifyTime to metadata of datumPayload", async () => {
    const payload = testDatumPayload;
    const newDoc = await addDoc({ db, payload });

    expect(newDoc.meta).toMatchObject({
      createTime: mockNow.toUTC().toString(),
      modifyTime: mockNow.toUTC().toString(),
    });
  });

  it("throws error if document with id already exists", async () => {
    const id = "existingId";
    const existingData = { _id: id, abc: 123 } as DataOnlyPayload;
    await db.insert(existingData, id);

    const attemptedNewPayload = {
      _id: id,
      newData: "but this won't get inserted",
    };

    await expect(
      addDoc({ db, payload: attemptedNewPayload })
    ).rejects.toThrowError();
    const dbDoc = await db.get(id);
    expect(dbDoc).toMatchObject(existingData);
    expect(dbDoc).not.toHaveProperty("newData");
  });

  it("fails if called twice because of duplicate id", async () => {
    await addDoc({ db, payload: testDatumPayload });
    await expect(
      addDoc({ db, payload: testDatumPayload })
    ).rejects.toThrowError();
  });

  it.skip("calls another document control method if id already exists and conflict strategy is given", async () => {
    fail();
  });
});

describe("overwriteDoc", () => {
  const dbName = "overwrite_doc_test";
  const db = testNano.db.use<EitherPayload>(dbName);

  beforeAll(async () => {
    await testNano.db.destroy(dbName).catch(pass);
  });

  beforeEach(async () => {
    await testNano.db.create(dbName).catch(pass);
    timezone_mock.register("UTC");
    Settings.now = () => mockNow.toMillis();
  });

  afterEach(async () => {
    await testNano.db.destroy(dbName).catch(pass);
    timezone_mock.unregister();
    Settings.resetCaches();
  });

  it("fails if id to be overwritten does not exist in db", async () => {
    await expect(
      overwriteDoc({ db, id: "does-not-exist", payload: { valid: "data" } })
    ).rejects.toThrowError();
    await expect(
      overwriteDoc({
        db,
        id: "does-not-exist",
        payload: { _id: "does-not-exist", data: "data" },
      })
    ).rejects.toThrowError();
    await expect(
      overwriteDoc({
        db,
        id: "does-not-exist",
        payload: { _id: "some-other-id", data: "data" },
      })
    ).rejects.toThrowError();
    await expect(
      overwriteDoc({
        db,
        id: "does-not-exist",
        payload: { data: { foo: "bar" }, meta: { idStructure: "%foo%" } },
      })
    ).rejects.toThrowError();
  });

  it("replaces the existing document if the new id is the same", async () => {
    await db.insert({ _id: "existing-id", oldKey: "oldData" });

    const newDoc1 = await overwriteDoc({
      db,
      id: "existing-id",
      payload: { _id: "existing-id", newKey1: "newData1" },
    });
    const dbDoc1 = await db.get("existing-id");
    expect(dbDoc1).toEqual(newDoc1);
    expect(newDoc1).toHaveProperty("newKey1", "newData1");
    expect(newDoc1).not.toHaveProperty("oldKey");

    const payload2 = {
      _id: "existing-id",
      data: { newKey2: "newData2" },
      meta: { humanId: "abcdefg" },
    };
    const newDoc2 = await overwriteDoc({
      db,
      id: "existing-id",
      payload: payload2,
    });
    const dbDoc2 = await db.get("existing-id");
    expect(dbDoc2).toEqual(newDoc2);
    expect(newDoc2).toMatchObject(payload2);
    expect(newDoc2).not.toHaveProperty("newKey1");

    const payload3 = {
      data: { newKey3: "newData3", idKey: "existing-id" },
      meta: { idStructure: "%idKey%" },
    };
    const newDoc3 = await overwriteDoc({
      db,
      id: "existing-id",
      payload: payload3,
    });
    const dbDoc3 = await db.get("existing-id");
    expect(dbDoc3).toEqual(newDoc3);
    expect(newDoc3).toMatchObject(payload3);
    expect(newDoc3).not.toHaveProperty("data.newKey2");
  });

  it("if new document does not have id, it replaces the doc at the old id", async () => {
    await db.insert({ _id: "existing-id", oldKey: "oldData" });

    const newDoc1 = await overwriteDoc({
      db,
      id: "existing-id",
      payload: { newKey: "newData" },
    });
    const dbDoc1 = await db.get("existing-id");
    expect(dbDoc1).toEqual(newDoc1);
    expect(newDoc1).toHaveProperty("newKey1", "newData1");
    expect(newDoc1).not.toHaveProperty("oldKey");

    const payload2 = {
      data: { newKey2: "newData2" },
      meta: { humanId: "abcdefg" },
    };
    const newDoc2 = await overwriteDoc({
      db,
      id: "existing-id",
      payload: payload2,
    });
    const dbDoc2 = await db.get("existing-id");
    expect(dbDoc2).toEqual(newDoc2);
    expect(newDoc2).toMatchObject(payload2);
    expect(newDoc2).not.toHaveProperty("newKey1");
  });

  it("deletes the old document if the new document has a different id", async () => {
    await db.insert({ _id: "old-id", oldKey: "oldData" });

    const payload1 = { _id: "new-id-1", newKey1: "newData1" };
    const newDoc1 = await overwriteDoc({ db, id: "old-id", payload: payload1 });
    await expect(db.get("old-id")).rejects.toThrow("deleted");
    const dbDoc1 = await db.get("new-id-1");
    expect(dbDoc1).toEqual(newDoc1);
    expect(newDoc1).toMatchObject(payload1);
    expect(newDoc1).not.toHaveProperty("oldKey");

    const payload2 = {
      _id: "new-id-2",
      data: { newKey2: "newData2" },
      meta: { humanId: "abcdefgh" },
    };
    const newDoc2 = await overwriteDoc({
      db,
      id: "new-id-1",
      payload: payload2,
    });
    await expect(db.get("new-id-1")).rejects.toThrow("deleted");
    const dbDoc2 = await db.get("new-id-2");
    expect(dbDoc2).toEqual(newDoc2);
    expect(newDoc2).toMatchObject(payload2);
    expect(newDoc2).not.toHaveProperty("newKey1");
    expect(newDoc2).not.toHaveProperty("data.newKey1");

    const payload3 = {
      data: { newKey3: "newData3", idKey: "new-id-3" },
      meta: { idStructure: "%idKey%" },
    };
    const newDoc3 = await overwriteDoc({
      db,
      id: "new-id-2",
      payload: payload3,
    });
    await expect(db.get("new-id-2")).rejects.toThrow("deleted");
    const dbDoc3 = await db.get("new-id-3");
    expect(dbDoc3).toEqual(newDoc3);
    expect(newDoc3).toMatchObject(payload3);
    expect(newDoc3).not.toHaveProperty("data.newKey2");
  });

  it("updates modifyTime to now for DatumPayloads", async () => {
    const notNow = DateTime.utc(2010, 11, 12, 13, 14, 15).toString();
    const now = mockNow.toString();
    const payloadWithoutModified = {data: {foo: "bar"}, meta: {occurTime: notNow}};
    const payloadWithModified = {data: {foo: "bar"}, meta: {occurTime: notNow, modifyTime: notNow}};


    await db.insert({_id: "data-only-payload-1", foo: "bar"});
    const newDoc1 = overwriteDoc({db, id: "data-only-payload-1", payload: payloadWithoutModified});
    await db.insert({_id: "data-only-payload-2", foo: "bar"});
    const newDoc2 = overwriteDoc({db, id: "data-only-payload-2", payload: payloadWithModified});

    await db.insert({_id: "datum-without-modifyTime-1", ...payloadWithoutModified});
    const newDoc3 = overwriteDoc({db, id: "datum-without-modifyTime-1", payload: payloadWithoutModified});
    await db.insert({_id: "datum-without-modifyTime-2", ...payloadWithoutModified});
    const newDoc4 = overwriteDoc({db, id: "datum-without-modifyTime-2", payload: payloadWithModified});

    await db.insert({_id: "datum-with-modifyTime-1", ...payloadWithModified});
    const newDoc5 = overwriteDoc({db, id: "datum-with-modifyTime-1", payload: payloadWithoutModified});
    await db.insert({_id: "datum-with-modifyTime-2", ...payloadWithModified});
    const newDoc6 = overwriteDoc({db, id: "datum-with-modifyTime-2", payload: payloadWithModified});

    for (const doc in [newDoc1, newDoc2, newDoc3, newDoc4, newDoc5, newDoc6]) {
      expect(doc).toHaveProperty("meta.modifyTime", now);
    }
  });

  it("if metadata exists on both documents it uses the createTime of the old document, but otherwise all other metadata from the new document", async () => {
    const timeA = DateTime.utc(2010,11, 12, 13, 14, 15).toString();
    const timeB = DateTime.utc(2013,12,11,10,9,8).toString();
    const oldDoc = {
      _id: "doc-id",
      data: {},
      meta: {
        occurTime: timeA,
        utcOffset: 1,
        createTime: timeA,
        humanId: "olddoc"
      }
    };
    const newPayload = {
      _id: "doc-id",
      data: {},
      meta: {
        occurTime: timeB,
        utcOffset: 2,
        createTime: timeB,
        humanId: "newdoc"
      }
    };
    const expectedNewDoc = {
      _id: "doc-id",
      data: {},
      meta: {
        occurTime: timeB,
        utcOffset: 2,
        createTime: timeA,
        humanId: "newdoc"
      }
    };
    await db.insert(oldDoc);
    const newDoc = await overwriteDoc({db, id: "doc-id", payload: newPayload});
    expect(newDoc).toMatchObject(expectedNewDoc);
  });

  it("if new doc is dataOnly, no metadata is saved from old doc", async () => {
    fail();
  });

  it("if createTime or metadata does not exist on old document, new document does not have a createTime because it is unknown", async () => {
    fail();
  });

  it("fails if new id clashes with a different document in the database", async () => {
    fail();
  })
});
