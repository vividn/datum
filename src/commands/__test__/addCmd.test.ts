import { fail, mockedLogLifecycle, testDbLifecycle } from "../../test-utils";
import { BaseDataError } from "../../errors";
import { DatumDocument } from "../../documentControl/DatumDocument";
import { addCmd } from "../addCmd";
import * as addDoc from "../../documentControl/addDoc";
import { DocExistsError } from "../../documentControl/base";
import SpyInstance = jest.SpyInstance;
import { Show } from "../../input/outputArgs";

describe("addCmd", () => {
  const dbName = "add_cmd_test";

  const db = testDbLifecycle(dbName);

  const mockedLog = mockedLogLifecycle();

  let addDocSpy: SpyInstance;
  beforeEach(() => {
    addDocSpy = jest.spyOn(addDoc, "addDoc");
  });

  it("inserts documents into couchdb", async () => {
    await addCmd({});

    await db.info().then((info) => {
      expect(info.doc_count).toEqual(1);
    });
  });

  it("calls addDoc", async () => {
    await addCmd({ idPart: "%foo%", data: ["foo=abc"] });
    const spyCall = addDocSpy.mock.calls[0][0];
    expect(spyCall).toMatchObject({
      db: db,
      payload: { data: { foo: "abc" }, meta: { idStructure: "%foo%" } },
    });
  });

  it("Can remove metadata entirely", async () => {
    expect(await addCmd({ idPart: "hasMetadata" })).toHaveProperty("meta");
    expect(
      await addCmd({ idPart: "noMeta", noMetadata: true })
    ).not.toHaveProperty("meta");
  });

  it("tells the user if the document already exists with identical data", async () => {
    await addCmd({
      idPart: "my name is bob",
      noTimestamp: true,
      data: ["foo=bar"],
      show: Show.Standard,
    });
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("CREATE"));
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("EXISTS")
    );

    mockedLog.mockReset();

    await addCmd({
      idPart: "my name is bob",
      noTimestamp: true,
      data: ["foo=bar"],
      show: Show.Standard,
    });
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("CREATE")
    );
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("EXISTS"));
  });

  it("fails if addedDocument conflicts with different data", async () => {
    await addCmd({
      idPart: "my name is doug",
      noTimestamp: true,
      data: ["foo=bar"],
      show: Show.Standard,
    });
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("CREATE"));
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("EXISTS")
    );

    mockedLog.mockReset();

    try {
      await addCmd({
        idPart: "my name is doug",
        noTimestamp: true,
        data: ["different=data"],
        show: Show.Standard,
      });
      fail();
    } catch (e) {
      expect(e).toBeInstanceOf(DocExistsError);
    }
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("FAILED"));
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
    const matchExtraKeysInAnyOrder =
      /^(?=[\s\S]*_id:)(?=[\s\S]*data:)(?=[\s\S]*meta:)/;
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

  it("can merge into an existing document with --merge", async () => {
    await addCmd({ idPart: "doc-id", data: ["foo=abc"] });
    const newDoc = await addCmd({
      idPart: "doc-id",
      data: ["foo=def"],
      merge: true,
    });
    expect(newDoc).toMatchObject({ data: { foo: ["abc", "def"] } });
    expect(addDocSpy).toHaveBeenCalledTimes(2);
    expect(addDocSpy.mock.calls[1][0].conflictStrategy).toEqual("merge");
  });

  it("can update and existing document with --conflict", async () => {
    await addCmd({ idPart: "doc-id", data: ["foo=abc"] });
    const newDoc = await addCmd({
      idPart: "doc-id",
      data: ["foo=def"],
      conflict: "preferNew",
    });
    expect(addDocSpy).toHaveBeenCalledTimes(2);
    expect(addDocSpy.mock.calls[1][0].conflictStrategy).toEqual("preferNew");
    expect(newDoc).toMatchObject({ data: { foo: "def" } });
  });

  // TODO: write tests for all of the various options

  it("stores the occurTime in the data", async () => {
    const newDoc = (await addCmd({
      date: "2021-08-23",
      time: "12",
      timezone: "0",
    })) as DatumDocument;
    expect(newDoc.data).toHaveProperty("occurTime", "2021-08-23T12:00:00.000Z");
    expect(newDoc.meta).not.toHaveProperty("occurTime");
  });

  it("stores the occurTime and utcOffset in DataOnly docs", async () => {
    const newDoc = (await addCmd({
      noMetadata: true,
      date: "2021-08-23",
      time: "12",
      timezone: "0",
    })) as DatumDocument;
    expect(newDoc).toHaveProperty("occurTime", "2021-08-23T12:00:00.000Z");
    expect(newDoc).toHaveProperty("occurUtcOffset", 0);
  });

  it("stores utcOffset", async () => {
    const newDoc = (await addCmd({
      date: "2021-08-23",
      time: "12",
      timezone: "0",
    })) as DatumDocument;
    expect(newDoc.data).toHaveProperty("occurUtcOffset", 0);
  });
});
