import {
  DataOnlyDocument,
  DataOnlyPayload,
  DatumDocument,
  DatumPayload,
  EitherPayload
} from "../../src/documentControl/DatumDocument";
import { DateTime, Settings } from "luxon";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "@jest/globals";
import { pass, testNano } from "../test-utils";
import timezone_mock from "timezone-mock";
import addDoc from "../../src/documentControl/addDoc";
import { IdError } from "../../src/errors";
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

  it("can still insert if _rev is given in the payload", async () => {
    const rev = "1-974bb250edb4c7da3b2f6459b2411873";
    const newDoc = await addDoc({
      db,
      payload: { ...testDatumPayload, _rev: rev },
    });
    expect(newDoc).toMatchObject(testDatumPayload);
    expect(newDoc._rev).not.toEqual(rev);
  });

  it("does not alter the payload", async () => {
    const payload1 = {
      _id: "abcd",
      _rev: "1-974bb250edb4c7da3b2f6459b2411873",
      data: {
        foo: "bar",
        abc: 123,
      },
      meta: {
        occurTime: "2021-06-20T14:00:00Z",
        utcOffset: 2,
      },
    };
    const payload2 = jClone(payload1);
    expect(payload1).toEqual(payload2);

    await addDoc({ db, payload: payload1 });

    expect(payload1).toEqual(payload2);
  });

  it.todo(
    "calls another document control method if id already exists and conflict strategy is given"
  );
});