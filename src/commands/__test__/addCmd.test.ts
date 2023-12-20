import {
  fail,
  mockedLogLifecycle,
  testDbLifecycle,
} from "../../__test__/test-utils";
import { BaseDataError, IdError } from "../../errors";
import { DatumDocument } from "../../documentControl/DatumDocument";
import { addCmd } from "../addCmd";
import * as addDoc from "../../documentControl/addDoc";
import { DocExistsError } from "../../documentControl/base";
import SpyInstance = jest.SpyInstance;
import { Show } from "../../input/outputArgs";

describe("addCmd", () => {
  const dbName = "add_cmd_test";

  const db = testDbLifecycle(dbName);

  const { mockedLog } = mockedLogLifecycle();

  let addDocSpy: SpyInstance;
  beforeEach(() => {
    addDocSpy = jest.spyOn(addDoc, "addDoc");
  });

  it("inserts documents into couchdb", async () => {
    await addCmd({ data: ["foo=bar"] });

    await db.info().then((info) => {
      expect(info.doc_count).toEqual(1);
    });
  });

  it("includes field in the data", async () => {
    const doc1 = await addCmd({ field: "field", data: [] });
    expect(doc1.data).toEqual({ field: "field" });
    const doc2 = await addCmd({ field: "field", data: ["foo=bar"] });
    expect(doc2.data).toEqual({ field: "field", foo: "bar" });
  });

  it("uses the first non explicitly assigned field in the data as field, since field is positional populuated automatically and could have data in it", async () => {
    const doc1 = await addCmd({ field: "foo=bar", data: ["dataField"] });
    expect(doc1.data).toEqual({ foo: "bar", field: "dataField" });
    const doc2 = await addCmd({
      field: "foo=bar",
      data: ["dataField", "another=parameter"],
    });
    expect(doc2.data).toEqual({
      foo: "bar",
      field: "dataField",
      another: "parameter",
    });
  });

  it("still handles field appropriately when there are required keys", async () => {
    const doc1 = await addCmd({
      required: "abc",
      field: "field",
      data: ["value"],
    });
    expect(doc1.data).toEqual({ abc: "value", field: "field" });

    const doc2 = await addCmd({
      required: ["a", "b"],
      field: "abc=ghi",
      data: ["first", "second", "third"],
    });
    expect(doc2.data).toEqual({
      a: "second",
      b: "third",
      field: "first",
      abc: "ghi",
    });
  });

  it("can skip the field with --fieldless", async () => {
    const doc = await addCmd({
      field: "actuallyData",
      fieldless: true,
      optional: "dataKey",
    });
    expect(doc.data).toEqual({ dataKey: "actuallyData" });
  });

  it("uses the field prop to populate the field key, but can also be specified again in the data", async () => {
    expect((await addCmd({ field: "fromProps", data: [] })).data).toEqual({
      field: "fromProps",
    });

    expect((await addCmd({ data: ["field=fromExtra"] })).data).toEqual({
      field: "fromExtra",
    });
    expect(
      (await addCmd({ field: "fromProps", data: ["field=fromExtra"] })).data,
    ).toEqual({ field: "fromExtra" });
  });

  it("throws an error if addCmd is called with no id and no data", async () => {
    await expect(addCmd({})).rejects.toThrow(IdError);
  });

  it("throws an IdError if data is provided, but the id is specified as an empty string", async () => {
    await expect(addCmd({ idPart: "", data: ["foo=bar"] })).rejects.toThrow(
      IdError,
    );
  });

  it("can add a blank document if an id is provided", async () => {
    const doc = await addCmd({ idPart: "test" });
    expect(doc._id).toEqual("test");
    expect(JSON.stringify(doc.data)).toBe("{}");
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
      await addCmd({ idPart: "noMeta", noMetadata: true }),
    ).not.toHaveProperty("meta");
  });

  it("tells the user if the document already exists with identical data", async () => {
    await addCmd({
      idPart: "my name is bob",
      data: ["foo=bar"],
      show: Show.Standard,
    });
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("CREATE"));
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("EXISTS"),
    );

    mockedLog.mockReset();

    await addCmd({
      idPart: "my name is bob",
      data: ["foo=bar"],
      show: Show.Standard,
    });
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("CREATE"),
    );
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("EXISTS"));
  });

  it("fails if addedDocument conflicts with different data", async () => {
    await addCmd({
      idPart: "my name is doug",
      data: ["foo=bar"],
      show: Show.Standard,
    });
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("CREATE"));
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("EXISTS"),
    );

    mockedLog.mockReset();

    try {
      await addCmd({
        idPart: "my name is doug",
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
      await addCmd({ idPart: ["rawString", "%foo%!!"], data: ["foo=abc"] }),
    ).toMatchObject({
      meta: { idStructure: "rawString__%foo%!!" },
    });
  });

  it("can use custom base data", async () => {
    expect(
      await addCmd({ baseData: "{a: 1, b:2, c:3 }", idPart: "basedata-doc1" }),
    ).toMatchObject({ data: { a: 1, b: 2, c: 3 }, _id: "basedata-doc1" });
  });

  it("can write payloads directly by specifying base-data and no-metadata", async () => {
    expect(
      await addCmd({
        noMetadata: true,
        baseData: "{a: 1, b:2, c:3}",
        idPart: "basedata-doc2",
      }),
    ).toMatchObject({
      _id: "basedata-doc2",
      a: 1,
      b: 2,
      c: 3,
    });
  });

  it("throws a BaseDataError if baseData is malformed", async () => {
    await expect(addCmd({ baseData: "string" })).rejects.toThrowError(
      BaseDataError,
    );
  });

  it("prefers the _id specified when in no-metadata mode", async () => {
    expect(
      await addCmd({
        noMetadata: true,
        baseData: "{ _id: payload-id }",
        idPart: "argument-id",
      }),
    ).toMatchObject({ _id: "payload-id" });
    expect(
      await addCmd({
        noMetadata: true,
        baseData: "{ _id: payload-id-2 }",
        idPart: "%keyId%",
        data: ["keyId=key-id"],
      }),
    ).toMatchObject({ _id: "payload-id-2" });
    expect(
      await addCmd({
        noMetadata: true,
        data: ["_id=posArgs-id"],
        idPart: "idPart-id",
      }),
    ).toMatchObject({ _id: "posArgs-id" });
  });

  it("does not contain idStructure in the metadata if id does not depend on values from data", async () => {
    const returnDoc = (await addCmd({ idPart: "notAField" })) as DatumDocument;
    expect(returnDoc._id).toBe("notAField");
    expect(returnDoc.meta).not.toHaveProperty("idStructure");
  });

  it("can display just the data of documents or the whole documents", async () => {
    const matchExtraKeysInAnyOrder =
      /^(?=[\s\S]*_id:)(?=[\s\S]*data:)(?=[\s\S]*meta:)/;
    await addCmd({ idPart: "this-id" });
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringMatching(matchExtraKeysInAnyOrder),
    );

    mockedLog.mockClear();

    await addCmd({ idPart: "that-id", showAll: true });
    expect(mockedLog).toHaveBeenCalledWith(
      expect.stringMatching(matchExtraKeysInAnyOrder),
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

  it.todo(
    "can do a start, end, occur, or switch command by using a trailing undefined argument",
  );

  // TODO: write tests for all of the various options
});
