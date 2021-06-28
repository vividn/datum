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
import { main } from "../src";
import { BaseDataError } from "../src/errors";
import { DatumDocument } from "../src/documentControl/DatumDocument";

const nano = testNano;
const originalLog = console.log;

describe("main", () => {
  const mockedLog = jest.fn();
  const db = nano.use("datum");

  beforeAll(async () => {
    await nano.db.destroy("datum").catch(pass);
  });

  beforeEach(async () => {
    await nano.db.create("datum").catch(pass);
    console.log = mockedLog;
  });

  afterEach(async () => {
    await nano.db.destroy("datum").catch(pass);
    console.log = originalLog;
    mockedLog.mockReset();
  });

  it("inserts documents into couchdb", async () => {
    await main({});

    const db = nano.use("datum");
    await db.info().then((info) => {
      expect(info.doc_count).toEqual(1);
    });
  });

  it("creates the database if it doesn't exist", async () => {
    await nano.db.destroy("datum").catch(pass);

    await main({});
    nano.db.list().then((body) => {
      expect(body.includes("datum"));
    });
  });

  it("can undo adding documents with a known id", async () => {
    await main({ idPart: "this_one_should_be_deleted" });
    await main({ idPart: "kept" });

    const db = nano.use("datum");
    await db.info().then((info) => {
      expect(info.doc_count).toEqual(2);
    });
    await db.get("this_one_should_be_deleted");
    await db.get("kept");

    mockedLog.mockReset();
    await main({ idPart: "this_one_should_be_deleted", undo: true });

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
    await main({ time: now });
    const insertedDoc = (await db.get(now)) as DatumDocument;
    expect(insertedDoc.meta.idStructure).toMatch(/%?occurTime%/);

    await main({ time: inAMinute, undo: true });
    await expect(db.get(now)).rejects.toThrowError("deleted");
  });

  // TODO: Make undo system more robust and more tested

  it("Can remove metadata entirely", async () => {
    expect(await main({ idPart: "hasMetadata" })).toHaveProperty("meta");
    expect(
      await main({ idPart: "noMeta", noMetadata: true })
    ).not.toHaveProperty("meta");
  });

  it("tells the user if the document already exists", async () => {
    await main({ idPart: "my name is bob" });
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("CREATE"));
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("EXISTS")
    );

    mockedLog.mockReset();

    await main({ idPart: "my name is bob" });
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("CREATE")
    );
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("EXISTS"));
  });

  it("inserts id structure into the metadata", async () => {
    expect(
      await main({ idPart: ["rawString", "%foo%!!"], _: ["foo=abc"] })
    ).toMatchObject({
      meta: { idStructure: "rawString__%foo%!!" },
    });
  });

  it("can use custom base data", async () => {
    expect(
      await main({ baseData: "{a: 1, b:2, c:3 }", idPart: "basedata-doc1" })
    ).toMatchObject({ data: { a: 1, b: 2, c: 3 }, _id: "basedata-doc1" });
  });

  it("can write payloads directly by specifying base-data and no-metadata", async () => {
    expect(
      await main({
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
    await expect(main({ baseData: "string" })).rejects.toThrowError(
      BaseDataError
    );
  });

  it("prefers the _id specified when in no-metadata mode", async () => {
    expect(
      await main({
        noMetadata: true,
        baseData: "{ _id: payload-id }",
        idPart: "argument-id",
      })
    ).toMatchObject({ _id: "payload-id" });
    expect(
      await main({
        noMetadata: true,
        baseData: "{ _id: payload-id-2 }",
        idPart: "%keyId%",
        _: ["keyId=key-id"],
      })
    ).toMatchObject({ _id: "payload-id-2" });
    expect(
      await main({
        noMetadata: true,
        _: ["_id=posArgs-id"],
        idPart: "idPart-id",
      })
    ).toMatchObject({ _id: "posArgs-id" });
  });

  it("does not contain idStructure in the metadata if id does not depend on values from data", async () => {
    const returnDoc = (await main({ idPart: "notAField" })) as DatumDocument;
    expect(returnDoc._id).toBe("notAField");
    expect(returnDoc.meta).not.toHaveProperty("idStructure");
  });

  it("contains random identifiers in the metadata", async () => {
    const doc = await main({});
    const { random, humanId } = doc?.meta;

    expect(random).toBeGreaterThanOrEqual(0);
    expect(random).toBeLessThanOrEqual(1);
    expect(humanId).toEqual(expect.stringMatching(/^[0-9a-z]+$/));
  });

  it("can display just the data of documents or the whole documents", async () => {
    const matchExtraKeysInAnyOrder = /^(?=[\s\S]*_id:)(?=[\s\S]*data:)(?=[\s\S]*meta:)/;
    await main({ idPart: "this-id" });
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringMatching(matchExtraKeysInAnyOrder)
    );

    mockedLog.mockClear();

    await main({ idPart: "that-id", showAll: true });
    expect(mockedLog).toHaveBeenCalledWith(
      expect.stringMatching(matchExtraKeysInAnyOrder)
    );
  });
});
