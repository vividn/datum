import { setNow, testDbLifecycle } from "../../__test__/test-utils";
import { setupCmd } from "../setupCmd";
import * as updateDoc from "../../documentControl/updateDoc";
import { EitherDocument } from "../../documentControl/DatumDocument";
import { updateCmd } from "../updateCmd";
import * as quickId from "../../ids/quickId";
import { mock } from "jest-mock-extended";
import { Show } from "../../input/outputArgs";
import { addCmd } from "../addCmd";
import { getCmd } from "../getCmd";
import { LastDocsTooOldError } from "../../errors";

describe("updateCmd", () => {
  const dbName = "update_cmd_test";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    await setupCmd("");
  });

  it("can update an existing doc from the first few letters of its humanId", async () => {
    await db.put({
      _id: "doc_to_update",
      data: { foo: "bar" },
      meta: { humanId: "abcdefg" },
    });
    const retDocs = await updateCmd(
      "abc --strategy update foo=baz newField=newData",
    );
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
    const retDocs = await updateCmd(
      "some_data_only --strategy merge -k foo -k newField= baz newData",
    );
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
    const quickIdSpy = jest
      .spyOn(quickId, "quickId")
      .mockImplementation(async () => ["quick_id"]);
    const updateDocSpy = jest
      .spyOn(updateDoc, "updateDoc")
      .mockReturnValue(Promise.resolve(updateDocReturn));

    const retDocs = await updateCmd("input_quick --strategy xor foo=bar");
    expect(retDocs).toHaveLength(1);
    expect(retDocs[0]).toBe(updateDocReturn);
    expect(quickIdSpy).toHaveBeenCalledWith("input_quick", expect.anything());
    expect(updateDocSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        id: "quick_id",
        updateStrategy: "xor",
        payload: { foo: "bar" },
      }),
    );
  });

  it("uses update as the default updateStrategy", async () => {
    jest.spyOn(quickId, "quickId").mockImplementation(async () => ["quick_id"]);
    const updateDocSpy = jest
      .spyOn(updateDoc, "updateDoc")
      .mockReturnValue(Promise.resolve(mock<EitherDocument>()));

    await updateCmd("input_quick foo=bar");
    expect(updateDocSpy).toHaveBeenCalledWith(
      expect.objectContaining({ updateStrategy: "update" }),
    );
  });

  it("outputs an UPDATE message or a NODIFF message when show is standard", async () => {
    const originalLog = console.log;
    const mockLog = jest.fn();
    console.log = mockLog;

    await db.put({ _id: "zzz", data: { foo: "bar" }, meta: {} });
    await updateCmd("zzz foo=baz", { show: Show.Standard });
    expect(mockLog).toHaveBeenCalledWith(expect.stringContaining("UPDATE"));
    mockLog.mockReset();

    await updateCmd("zzz foo=baz", { show: Show.Standard });
    expect(mockLog).toHaveBeenCalledWith(expect.stringContaining("NODIFF"));

    console.log = originalLog;
  });

  it("can update multiple documents with a compound quickId", async () => {
    await db.put({ _id: "zzz", data: { foo: "bar", bar: "foo" }, meta: {} });
    await db.put({ _id: "yyy", data: { foo: "bar", bar: "foo2" }, meta: {} });
    const returnValue = await updateCmd(",zzz,yyy foo=baz newField=newData");
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
    const retDocs = await updateCmd("foo=baz abcd another=thing");
    expect(retDocs).toHaveLength(1);
    expect(retDocs[0]).toMatchObject({
      _id: "doc_to_update",
      data: { foo: "baz", another: "thing" },
      meta: { humanId: "abcdefg" },
    });
  });

  it("can update a key with an undefined value easily", async () => {
    const { _id } = await addCmd("field foo=bar baz=qux --id foobar");
    const retDocs = await updateCmd(`${_id} foo=`);
    expect(retDocs).toHaveLength(1);
    expect(retDocs[0].data).toEqual({
      field: "field",
      baz: "qux",
    });
    expect(retDocs[0].data.foo).toBeUndefined();
  });

  it("can update the last added document when no quickId is provided", async () => {
    const { _id } = await addCmd("field foo=bar --id %foo");
    const retDocs = await updateCmd("foo=baz");
    expect(retDocs).toHaveLength(1);
    expect(retDocs[0]._id).not.toEqual(_id);
    const newId = "field:baz";
    expect(retDocs[0]._id).toEqual(newId);
    expect(retDocs[0].data).toEqual({
      field: "field",
      foo: "baz",
    });
    const dbDoc = await db.get(newId);
    expect(dbDoc).toEqual(retDocs[0]);
  });

  it("can update all the of documents returned by getCmd at once", async () => {
    // TODO: get docs based off of data once that is possible
    const {
      _id: id1,
      meta: { humanId: hid1 },
    } = await addCmd("field foo=bar");
    const {
      _id: id2,
      meta: { humanId: hid2 },
    } = await addCmd("field bar=foo");
    await getCmd(`${hid1},${hid2},`);
    const retDocs = await updateCmd("foo=baz");
    expect(retDocs).toHaveLength(2);
    expect(retDocs[0]).toMatchObject({
      _id: id1,
      data: { foo: "baz" },
    });
    expect(retDocs[1]).toMatchObject({
      _id: id2,
      data: { bar: "foo", foo: "baz" },
    });

    const dbDoc1 = await db.get(id1);
    const dbDoc2 = await db.get(id2);
    expect(retDocs[0]).toEqual(dbDoc1);
    expect(retDocs[1]).toEqual(dbDoc2);
  });

  it("won't update the last doc if the ref is more than 15 minutes old", async () => {
    setNow("2024-05-10, 15:40");
    await addCmd("field foo=bar --id %foo");
    setNow("+16");
    await expect(updateCmd("foo=baz")).rejects.toThrow(LastDocsTooOldError);
  });
});
