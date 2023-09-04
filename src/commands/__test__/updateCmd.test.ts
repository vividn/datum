import { testDbLifecycle } from "../../test-utils";
import { setupCmd } from "../setupCmd";
import * as updateDoc from "../../documentControl/updateDoc";
import { EitherDocument } from "../../documentControl/DatumDocument";
import { updateCmd } from "../updateCmd";
import * as quickId from "../../ids/quickId";
import { mock } from "jest-mock-extended";
import { Show } from "../../input/outputArgs";

describe("updateCmd", () => {
  const dbName = "update_cmd_test";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    await setupCmd({ db: dbName });
  });

  it("can update an existing doc from the first few letters of its humanId", async () => {
    await db.put({
      _id: "doc_to_update",
      data: { foo: "bar" },
      meta: { humanId: "abcdefg" },
    });
    const retDocs = await updateCmd({
      db: dbName,
      quickId: "abc",
      strategy: "preferNew",
      data: ["foo=baz", "newField=newData"],
    });
    expect(retDocs).toHaveLength(1);
    const retDoc = retDocs[0];
    const dbDoc = await db.get("doc_to_update");
    expect(retDoc).toEqual(dbDoc);
    expect(retDoc).toMatchObject({
      _id: "doc_to_update",
      data: { foo: "baz", newField: "newData" },
    });
  });

  it("can update a dataonly doc from the first letters of its id", async () => {
    await db.put({ _id: "some_data_only", foo: "bar" });
    const retDocs = await updateCmd({
      db: dbName,
      quickId: "some",
      strategy: "merge",
      required: ["foo"],
      optional: ["newField"],
      data: ["baz", "newData"],
    });
    expect(retDocs).toHaveLength(1);
    const dbDoc = await db.get("some_data_only");
    expect(retDocs[0]).toEqual(dbDoc);
    expect(retDocs[0]).toMatchObject({
      _id: "some_data_only",
      foo: ["bar", "baz"],
      newField: "newData",
    });
  });

  it("calls quickId and updateDoc", async () => {
    const updateDocReturn = mock<EitherDocument>();
    const quickIdsSpy = jest
      .spyOn(quickId, "quickIds")
      .mockImplementation(async () => ["quick_id"]);
    const updateDocSpy = jest
      .spyOn(updateDoc, "updateDoc")
      .mockReturnValue(Promise.resolve(updateDocReturn));

    const retDocs = await updateCmd({
      db: dbName,
      quickId: "input_quick",
      strategy: "xor",
      data: ["foo=bar"],
    });
    expect(retDocs).toHaveLength(1);
    expect(retDocs[0]).toBe(updateDocReturn);
    expect(quickIdsSpy).toHaveBeenCalledWith(expect.anything(), "input_quick");
    expect(updateDocSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        id: "quick_id",
        updateStrategy: "xor",
        payload: { foo: "bar" },
      })
    );
  });

  it("uses preferNew as the default updateStrategy", async () => {
    jest
      .spyOn(quickId, "quickIds")
      .mockImplementation(async () => ["quick_id"]);
    const updateDocSpy = jest
      .spyOn(updateDoc, "updateDoc")
      .mockReturnValue(Promise.resolve(mock<EitherDocument>()));

    await updateCmd({ db: dbName, quickId: "input_quick", data: ["foo=bar"] });
    expect(updateDocSpy).toHaveBeenCalledWith(
      expect.objectContaining({ updateStrategy: "preferNew" })
    );
  });

  it("outputs an UPDATE message or a NODIFF message when show is standard", async () => {
    const originalLog = console.log;
    const mockLog = jest.fn();
    console.log = mockLog;

    await db.put({ _id: "zzz", data: { foo: "bar" }, meta: {} });
    await updateCmd({
      db: dbName,
      quickId: "zzz",
      data: ["foo=baz"],
      show: Show.Standard,
    });
    expect(mockLog).toHaveBeenCalledWith(expect.stringContaining("UPDATE"));
    mockLog.mockReset();

    await updateCmd({
      db: dbName,
      quickId: "zzz",
      data: ["foo=baz"],
      show: Show.Standard,
    });
    expect(mockLog).toHaveBeenCalledWith(expect.stringContaining("NODIFF"));

    console.log = originalLog;
  });

  it("can update multiple documents with a compound quickId", async () => {
    await db.put({ _id: "zzz", data: { foo: "bar", bar: "foo" }, meta: {} });
    await db.put({ _id: "yyy", data: { foo: "bar", bar: "foo2" }, meta: {} });
    const returnValue = await updateCmd({
      db: dbName,
      quickId: ",zzz,yyy",
      data: ["foo=baz", "newField=newData"],
    });
    const zzzMatchObject = {
      _id: "zzz",
      data: { foo: "baz", bar: "foo", newField: "newData" },
    };
    const yyyMatchObject = {
      _id: "yyy",
      data: { foo: "baz", bar: "foo2", newField: "newData" },
    };
    expect(returnValue).toHaveLength(2);
    expect(returnValue[0]).toMatchObject(zzzMatchObject);
    expect(returnValue[1]).toMatchObject(yyyMatchObject);

    const zzz = await db.get("zzz");
    const yyy = await db.get("yyy");
    expect(zzz).toMatchObject(zzzMatchObject);
    expect(yyy).toMatchObject(yyyMatchObject);
  });

  it("can take a data specification before the quick id argument by using an ='s notation", async () => {
    // TODO: rewrite with cmd string notation once supported
    await db.put({
      _id: "doc_to_update",
      data: { foo: "bar" },
      meta: { humanId: "abcdefg" },
    });
    const retDocs = await updateCmd({
      quickId: "foo=baz",
      data: ["abcd", "another=thing"],
    });
    expect(retDocs).toHaveLength(1);
    expect(retDocs[0]).toMatchObject({
      _id: "doc_to_update",
      data: { foo: "baz", another: "thing" },
      meta: { humanId: "abcdefg" },
    });
  });
});
