import {
  deterministicHumanIds,
  fail,
  mockedLogLifecycle,
  restoreNow,
  setNow,
  testDbLifecycle,
} from "../../__test__/test-utils";
import { BaseDataError, IdError } from "../../errors";
import { DatumDocument } from "../../documentControl/DatumDocument";
import { addCmd } from "../addCmd";
import * as addDoc from "../../documentControl/addDoc";
import { DocExistsError } from "../../documentControl/base";
import SpyInstance = jest.SpyInstance;
import { setupCmd } from "../setupCmd";

describe("addCmd", () => {
  const dbName = "add_cmd_test";

  const db = testDbLifecycle(dbName);

  const { mockedLog } = mockedLogLifecycle();
  deterministicHumanIds();

  let addDocSpy: SpyInstance;
  beforeEach(() => {
    addDocSpy = jest.spyOn(addDoc, "addDoc");
  });

  it("inserts documents into couchdb", async () => {
    await addCmd("foo");

    await db.info().then((info) => {
      expect(info.doc_count).toEqual(1);
    });
  });

  it("includes field in the data", async () => {
    const doc1 = await addCmd("field");
    expect(doc1.data).toEqual({ field: "field" });
    const doc2 = await addCmd("field foo=bar");
    expect(doc2.data).toEqual({ field: "field", foo: "bar" });
  });

  it("uses the first non explicitly assigned field in the data as field", async () => {
    const doc1 = await addCmd("foo=bar dataField");
    expect(doc1.data).toEqual({ foo: "bar", field: "dataField" });
    const doc2 = await addCmd("foo=bar dataField another=parameter");
    expect(doc2.data).toEqual({
      foo: "bar",
      field: "dataField",
      another: "parameter",
    });
  });

  it("still handles field appropriately when there are required keys", async () => {
    const doc1 = await addCmd("-K abc field value");
    expect(doc1.data).toEqual({ abc: "value", field: "field" });

    const doc2 = await addCmd("-K a -K b abc=ghi first second third");
    expect(doc2.data).toEqual({
      a: "second",
      b: "third",
      field: "first",
      abc: "ghi",
    });
  });

  it("can skip the field with -F", async () => {
    const doc = await addCmd("-F -k dataKey actuallyData");
    expect(doc.data).toEqual({ dataKey: "actuallyData" });
  });

  it("uses the field prop to populate the field key, but can also be specified again in the data", async () => {
    expect((await addCmd("-F field=manual")).data).toEqual({
      field: "manual",
    });
    expect((await addCmd("fromField field=fromData")).data).toEqual({
      field: "fromData",
    });
  });

  it("throws an error if addCmd is called with no field, no id, and no data", async () => {
    await expect(addCmd("-F")).rejects.toThrow(IdError);
  });

  it("throws an IdError if data is provided, but the id is specified as an empty string", async () => {
    await expect(addCmd("-F data=data --id ''")).rejects.toThrow(IdError);
  });

  it("can add a blank document if an id is provided", async () => {
    const doc = await addCmd("-F --id test");
    expect(doc._id).toEqual("test");
    expect(JSON.stringify(doc.data)).toBe("{}");
  });

  it("calls addDoc", async () => {
    await addCmd("field foo=abc --id %foo%");
    const spyCall = addDocSpy.mock.calls[0][0];
    expect(spyCall).toMatchObject({
      db: db,
      payload: {
        data: { foo: "abc", field: "field" },
        meta: { idStructure: "%field%:%foo%" },
      },
    });
  });

  it("Can remove metadata entirely", async () => {
    expect(await addCmd("-F --id hasMetadata")).toHaveProperty("meta");
    expect(await addCmd("-FM --id noMetadata")).not.toHaveProperty("meta");
  });

  it("tells the user if the document already exists identical data", async () => {
    await addCmd("--id 'my name is bob' foo=bar field --show standard");
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("CREATE"));
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("EXISTS")
    );

    mockedLog.mockReset();

    await addCmd("--id 'my name is bob' foo=bar field --show standard");
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("CREATE")
    );
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("EXISTS"));
  });

  it("fails if addedDocument conflicts with different data", async () => {
    await addCmd("--id 'my name is doug' foo=bar field --show standard");
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("CREATE"));
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringContaining("EXISTS")
    );

    mockedLog.mockReset();

    try {
      await addCmd(
        "--id 'my name is doug' different=data field --show standard"
      );
      fail();
    } catch (e) {
      expect(e).toBeInstanceOf(DocExistsError);
    }
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("FAILED"));
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("EXISTS"));
  });

  it("inserts id structure into the metadata", async () => {
    expect(
      await addCmd("--id rawString --id %foo%!! foo=abc field")
    ).toMatchObject({
      meta: { idStructure: "%field%:rawString__%foo%!!" },
    });
  });

  it("can use custom base data", async () => {
    expect(
      await addCmd('-b "{a: 1, b:2, c:3 }" field --id "basedata-doc1"')
    ).toMatchObject({
      data: { a: 1, b: 2, c: 3, field: "field" },
      _id: "field:basedata-doc1",
    });
  });

  it("can write payloads directly by specifying base-data no-metadata and fieldless", async () => {
    expect(
      await addCmd("-FM -b '{a: 1, b:2, c:3 }' --id basedata-doc2")
    ).toEqual({
      _id: "basedata-doc2",
      _rev: expect.any(String),
      a: 1,
      b: 2,
      c: 3,
    });
  });

  it("throws a BaseDataError if baseData is malformed", async () => {
    await expect(
      addCmd("-F -b 'string_is_not_good_basedata'")
    ).rejects.toThrowError(BaseDataError);
  });

  it("prefers the _id specified when in no-metadata mode", async () => {
    expect(
      await addCmd("-FM -b '{_id: payload-id}' --id argument-id")
    ).toMatchObject({ _id: "payload-id" });
    expect(
      await addCmd("-FM -b '{_id: payload-id-2}' --id '%keyId%' keyId=key-id")
    ).toMatchObject({ _id: "payload-id-2" });
    expect(await addCmd("-FM --id 'idPart-id' _id=posArgs-id")).toMatchObject({
      _id: "posArgs-id",
    });
  });

  it("does not contain idStructure in the metadata if id does not depend on values from data", async () => {
    const returnDoc = (await addCmd("-F --id rawString")) as DatumDocument;
    expect(returnDoc._id).toBe("rawString");
    expect(returnDoc.meta).not.toHaveProperty("idStructure");
  });

  it("can display just the data of documents or the whole documents", async () => {
    const matchExtraKeysInAnyOrder =
      /^(?=[\s\S]*_id:)(?=[\s\S]*data:)(?=[\s\S]*meta:)/;
    await addCmd("field --id this-id"); //note when called in tests like this show is "none" by default. From the main entrypoint it is "standard"
    expect(mockedLog).not.toHaveBeenCalledWith(
      expect.stringMatching(matchExtraKeysInAnyOrder)
    );

    mockedLog.mockClear();

    await addCmd("field --id that-id --show-all");
    expect(mockedLog).toHaveBeenCalledWith(
      expect.stringMatching(matchExtraKeysInAnyOrder)
    );

    mockedLog.mockClear();

    await addCmd("field --id short-show-all -A");
    expect(mockedLog).toHaveBeenCalledWith(
      expect.stringMatching(matchExtraKeysInAnyOrder)
    );
  });

  it("can merge into an existing document with --merge", async () => {
    await addCmd("-F --id doc-id foo=abc");
    const newDoc = await addCmd("-F --id doc-id foo=def --merge");
    expect(newDoc).toMatchObject({ data: { foo: ["abc", "def"] } });
    expect(addDocSpy).toHaveBeenCalledTimes(2);
    expect(addDocSpy.mock.calls[1][0].conflictStrategy).toEqual("merge");
  });

  it("can update and existing document with --conflict", async () => {
    await addCmd("-F --id doc-id foo=abc");
    const newDoc = await addCmd("-F --id doc-id foo=def --conflict preferNew");
    expect(addDocSpy).toHaveBeenCalledTimes(2);
    expect(addDocSpy.mock.calls[1][0].conflictStrategy).toEqual("preferNew");
    expect(newDoc).toMatchObject({ data: { foo: "def" } });
  });

  describe("change command", () => {
    beforeEach(async () => {
      setNow("2023-12-21 14:00");
      await setupCmd("");
    });
    afterAll(() => {
      restoreNow();
    });

    it("can become an occur command by having start as a trailing word", async () => {
      expect(
        await addCmd("field -K req1 -k opt1 reqVal optVal occur")
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become a start command by having start as a trailing word", async () => {
      expect(
        await addCmd("field -K req1 -k opt1 reqVal optVal start '30 min'")
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become an end command by having start as a trailing word", async () => {
      expect(
        await addCmd("field -K req1 -k opt1 reqVal optVal end '30 min'")
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become a switch command by having start as a trailing word", async () => {
      expect(
        await addCmd(
          "field -K req1 -k opt1 reqVal optVal switch stateName 5m30s"
        )
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });
  });
});
