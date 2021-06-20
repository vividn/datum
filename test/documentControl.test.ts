import {
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  jest,
} from "@jest/globals";

import { fail, pass, testNano } from "./test-utils";
import addDoc from "../src/documentControl/addDoc";
import {
  DataOnlyDocument, DataOnlyPayload,
  DatumDocument,
  DatumPayload,
} from "../src/documentControl/DatumDocument";
import timezone_mock from "timezone-mock";
import { DateTime, Settings } from "luxon";
import { GenericObject } from "../src/GenericObject";

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
  const dbName = "addDocTest";
  const db = testNano.db.use<DatumPayload | DataOnlyPayload>(dbName);

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

  it("it adds dataOnly payloads to the given database with the given id", async () => {
    const payload = { a: 1, c: "dataString" };
    const id = "dataOnlyPayloadId";

    const returnedDoc = await addDoc({ db, payload, id });
    const dbDoc = (await db.get(id)) as DataOnlyDocument;
    expect(returnedDoc).toMatchObject(dbDoc);
    expect(dbDoc).toMatchObject({ _id: id, ...payload });
  });

  it("throws error if trying to add dataOnly payload without id", async () => {
    const payload = { a: 1, c: "dataString" };
    await expect(addDoc({ db, payload })).toThrowError();
  });

  it("adds a datum payload to the database with its calculated id", async () => {
    const payload = testDatumPayload;
    const returnedDoc = await addDoc({ db, payload });
    const dbDoc = (await db.get(testDatumPayloadId)) as DatumDocument;

    expect(returnedDoc).toMatchObject(dbDoc);
    expect(dbDoc).toMatchObject(testDatumPayload);
  });

  it("adds a datum payload of calculated and given id match", async () => {
    const payload = testDatumPayload;
    const id = testDatumPayloadId;

    await addDoc({ db, payload, id });

    expect(await db.get(testDatumPayloadId)).toMatchObject(testDatumPayload);
  });

  it("throws an error if calculated id does not match a given id for a datum payload", async () => {
    const payload = testDatumPayload;
    const id = "non-matching-id";

    await expect(addDoc({ db, payload, id })).toThrowError();
  });

  it("adds createTime and modifyTime to metadata of datumPayload", async () => {
    const payload = testDatumPayload;
    const newDoc = await addDoc({db, payload});

    expect(newDoc.meta).toMatchObject({createTime: mockNow.toUTC().toString(), modifyTime: mockNow.toUTC().toString()});
  });

  it("throws error if document with id already exists", async () => {
    const id = "existingId";
    const existingData = {abc: 123} as DataOnlyPayload;
    await db.insert(existingData, id);

    const attemptedNewPayload = {newData: "but this won't get inserted"};

    await expect(addDoc({db, payload: attemptedNewPayload, id})).toThrowError();
    expect(await db.get(id)).toMatchObject(existingData);
  });

  it("fails if called twice because of duplicate id", async () => {
    await addDoc({db, payload: testDatumPayload});
    await expect(addDoc({db, payload: testDatumPayload})).toThrowError();
  });

  it.skip("calls another document control method if id already exists and conflict strategy is given", async () => {
    fail();
  });
});
