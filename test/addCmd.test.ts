import {
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  jest,
} from "@jest/globals";
import { pass, testNano } from "./test-utils";
import { BaseDataError } from "../src/errors";
import { DatumDocument, EitherPayload } from "../src/documentControl/DatumDocument";
import addCmd from "../src/commands/addCmd";
import * as connectDb from "../src/auth/connectDb";
import { DocumentScope } from "nano";

const originalLog = console.log;

describe("addCmd", () => {
  const mockedLog = jest.fn();
  const dbName = "add_cmd_test";
  const db = testNano.use(dbName) as DocumentScope<EitherPayload>;
  jest.spyOn(connectDb, "default").mockImplementation(() => db);

  beforeAll(async () => {
    await testNano.db.destroy(dbName).catch(pass);
  });

  beforeEach(async () => {
    await testNano.db.create(dbName).catch(pass);
    console.log = mockedLog;
  });

  afterEach(async () => {
    await testNano.db.destroy(dbName).catch(pass);
    console.log = originalLog;
    mockedLog.mockReset();
  });

  it("inserts documents into couchdb", async () => {
    await addCmd({});

    await db.info().then((info) => {
      expect(info.doc_count).toEqual(1);
    });
  });

  it("can undo adding documents with a known id", async () => {
    await addCmd({ idPart: "this_one_should_be_deleted" });
    await addCmd({ idPart: "kept" });

    await db.info().then((info) => {
      expect(info.doc_count).toEqual(2);
    });
    await db.get("this_one_should_be_deleted");
    await db.get("kept");

    mockedLog.mockReset();
    await addCmd({ idPart: "this_one_should_be_deleted", undo: true });

    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("DELETE"));
    await db.info().then((info) => {
      expect(info.doc_count).toEqual(1);
    });
    await db.get("kept");
    await expect(db.get("this_one_should_be_deleted")).rejects.toThrowError(
      "deleted"
    );
  });

  it("undoes a document with a time in the past if it contains occurTime", async () => {
    const now = "2021-06-28T06:30:00.000Z";
    const inAMinute = "2021-06-28T06:31:00.000Z";
    await addCmd({ time: now });
    const insertedDoc = (await db.get(now)) as DatumDocument;
    expect(insertedDoc.meta.idStructure).toMatch(/%?occurTime%/);

    await addCmd({ time: inAMinute, undo: true });
    await expect(db.get(now)).rejects.toThrowError("deleted");
  });

  // TODO: Make undo system more robust and more tested

  it("Can remove metadata entirely", async () => {
    expect(await addCmd({ idPart: "hasMetadata" })).toHaveProperty("meta");
    expect(
      await addCmd({ idPart: "noMeta", noMetadata: true })
    ).not.toHaveProperty("meta");
  });

  it("tells the user if the document already exists", async () => {
    await addCmd({ idPart: "my name is bob" });
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("CREATE"));
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("EXISTS")
    );

    mockedLog.mockReset();

    await addCmd({ idPart: "my name is bob" });
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("CREATE")
    );
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("EXISTS"));
  });

  it("inserts id structure into the metadata", async () => {
    expect(
      await addCmd({ idPart: ["rawString", "%foo%!!"], data: ["foo=abc"] })
    ).toMatchObject({
      meta: { idStructure: "rawString__%foo%!!" },
    });
  });

  it("can use custom base data", async () => {
    expect(
      await addCmd({ baseData: "{a: 1, b:2, c:3 }", idPart: "basedata-doc1" })
    ).toMatchObject({ data: { a: 1, b: 2, c: 3 }, _id: "basedata-doc1" });
  });

  it("can write payloads directly by specifying base-data and no-metadata", async () => {
    expect(
      await addCmd({
        noMetadata: true,
        baseData: "{a: 1, b:2, c:3}",
        idPart: "basedata-doc2",
      })
    ).toMatchObject({
      _id: "basedata-doc2",
      a: 1,
      b: 2,
      c: 3,
    });
  });

  it("throws a BaseDataError if baseData is malformed", async () => {
    await expect(addCmd({ baseData: "string" })).rejects.toThrowError(
      BaseDataError
    );
  });

  it("prefers the _id specified when in no-metadata mode", async () => {
    expect(
      await addCmd({
        noMetadata: true,
        baseData: "{ _id: payload-id }",
        idPart: "argument-id",
      })
    ).toMatchObject({ _id: "payload-id" });
    expect(
      await addCmd({
        noMetadata: true,
        baseData: "{ _id: payload-id-2 }",
        idPart: "%keyId%",
        data: ["keyId=key-id"],
      })
    ).toMatchObject({ _id: "payload-id-2" });
    expect(
      await addCmd({
        noMetadata: true,
        data: ["_id=posArgs-id"],
        idPart: "idPart-id",
      })
    ).toMatchObject({ _id: "posArgs-id" });
  });

  it("does not contain idStructure in the metadata if id does not depend on values from data", async () => {
    const returnDoc = (await addCmd({ idPart: "notAField" })) as DatumDocument;
    expect(returnDoc._id).toBe("notAField");
    expect(returnDoc.meta).not.toHaveProperty("idStructure");
  });

  it("contains random identifiers in the metadata", async () => {
    const doc = await addCmd({});
    const { random, humanId } = doc?.meta;

    expect(random).toBeGreaterThanOrEqual(0);
    expect(random).toBeLessThanOrEqual(1);
    expect(humanId).toEqual(expect.stringMatching(/^[0-9a-z]+$/));
  });

  it("can display just the data of documents or the whole documents", async () => {
    const matchExtraKeysInAnyOrder = /^(?=[\s\S]*_id:)(?=[\s\S]*data:)(?=[\s\S]*meta:)/;
    await addCmd({ idPart: "this-id" });
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringMatching(matchExtraKeysInAnyOrder)
    );

    mockedLog.mockClear();

    await addCmd({ idPart: "that-id", showAll: true });
    expect(mockedLog).toHaveBeenCalledWith(
      expect.stringMatching(matchExtraKeysInAnyOrder)
    );
  });
});
