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
  DataOnlyDocument,
  DatumDocument,
  DatumPayload,
} from "../src/documentControl/DatumDocument";

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

describe("addDoc", () => {
  const dbName = "addDocTest";
  const db = testNano.db.use<DatumDocument | DataOnlyDocument>(dbName);

  beforeAll(async () => {
    await testNano.db.destroy(dbName).catch(pass);
  });

  beforeEach(async () => {
    await testNano.db.create(dbName).catch(pass);
  });

  afterEach(async () => {
    await testNano.db.destroy(dbName).catch(pass);
  });

  it("it adds dataOnly payloads to the given database with the given id", async () => {
    const payload = { a: 1, c: "dataString" };
    const id = "dataOnlyPayloadId";

    await addDoc({ db, payload, id });

    expect(await db.get(id)).toMatchObject({ _id: id, ...payload });
  });

  it("throws error if trying to add dataOnly payload without id", async () => {
    const payload = { a: 1, c: "dataString" };
    await expect(addDoc({ db, payload })).toThrowError();
  });

  it("adds a datum payload to the database with its calculated id", async () => {
    const payload = testDatumPayload;
    await addDoc({db, payload});
    expect(await db.get(testDatumPayloadId)).toMatchObject(testDatumPayload);
  });

  it("adds a datum payload of calculated and given id match", async () => {
    const payload = testDatumPayload;
    const id = testDatumPayloadId;

    await addDoc({db, payload, id});

    expect(await db.get(testDatumPayloadId)).toMatchObject(testDatumPayload);
  });

  it("throws an error if calculated id does not match a given id for a datum payload", async () => {
    const payload = testDatumPayload;
    const id = "non-matching-id"

    await expect(addDoc({db, payload, id})).toThrowError();
  });

  it("adds createTime and modifyTime to metadata of datumPayload", async () => {
    fail();
  });

  it("throws error if document with id already exists", async () => {
    fail();
  });

  it("calls another document control method if id already exists and conflict strategy is given", async () => {
    fail();
  });
});
