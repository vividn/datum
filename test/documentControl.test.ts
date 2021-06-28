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
    expect(returnedDoc).toMatchObject(dbDoc);
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
    expect(returnedDoc).toMatchObject(dbDoc);
    expect(dbDoc).toMatchObject(payload);
  });

  it("adds a datum payload to the database with its calculated id", async () => {
    const payload = testDatumPayload;
    const returnedDoc = await addDoc({ db, payload });
    const dbDoc = (await db.get(testDatumPayloadId)) as DatumDocument;

    expect(returnedDoc).toMatchObject(dbDoc);
    expect(dbDoc).toMatchObject(testDatumPayload);
  });

  it("prefers to use the id calculated from structure", async () => {
    const payload = testDatumPayload;

    const returnedDoc = await addDoc({ db, payload });
    const dbDoc = (await db.get(testDatumPayloadId)) as DatumDocument;
    expect(returnedDoc).toMatchObject(dbDoc);
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
    await expect(overwriteDoc({db, id: "does-not-exist", payload: {valid: "data"} })).rejects.toThrowError();
    await expect(overwriteDoc({db, id: "does-not-exist", payload: {_id: "does-not-exist", data: "data"} })).rejects.toThrowError();
    await expect(overwriteDoc({db, id: "does-not-exist", payload: {_id: "some-other-id", data: "data" } })).rejects.toThrowError();
    await expect(overwriteDoc({db, id: "does-not-exist", payload: {data: {foo: "bar"}, meta: {idStructure: "%foo%"} }})).rejects.toThrowError();
  });

  it("replaces the existing document if the new id is the same", async () => {
    fail();
  });

  it("if new document does not have id, it replaces the doc at the old id", async () => {
    fail();
  });

  it("deletes the old document if the new document has a different id", async () => {
    fail();
  });

  it("updates modifiedTime to now for DatumPayloads", async () => {
    fail();
  });

  it("if metadata exists on both documents it uses the createTime of the old document, but otherwise all other metadata from the new document", async () => {
    fail();
  });

  it("if new doc is dataOnly, no metadata is saved from old doc", async () => {
    fail();
  });

  it("if createTime or metadata does not exist on old document, new document does not have a createTime because it is unknown", async () => {
    fail();
  });
});
