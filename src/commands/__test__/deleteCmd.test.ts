import { testDbLifecycle } from "../../test-utils";
import * as deleteDoc from "../../documentControl/deleteDoc";
import { deleteCmd } from "../deleteCmd";
import * as quickId from "../../ids/quickId";
import { setupCmd } from "../setupCmd";
import { Show } from "../../input/outputArgs";

describe("deleteCmd", () => {
  const dbName = "delete_cmd_test";
  const db = testDbLifecycle(dbName);

  let deleteDocSpy: any;
  beforeEach(async () => {
    deleteDocSpy = jest.spyOn(deleteDoc, "deleteDoc");
    await setupCmd({ db: dbName });
  });

  it("deletes a document based on first few letters of humanId", async () => {
    await db.put({ _id: "hello", data: {}, meta: { humanId: "a44quickId" } });
    const returned = await deleteCmd({ db: dbName, quickId: "a44" });

    expect(returned).toEqual([
      expect.objectContaining({ _id: "hello", _deleted: true }),
    ]);
    expect(deleteDocSpy).toHaveBeenCalledWith(
      expect.objectContaining({ id: "hello" })
    );
    await expect(db.get("hello")).rejects.toMatchObject({
      name: "not_found",
      reason: "deleted",
    });
  });

  it("deletes a document based on first few letters of _id", async () => {
    await db.put({ _id: "the_quick_brown_fox", foo: "abc" });
    const returned = await deleteCmd({ db: dbName, quickId: "the_qu" });

    expect(returned).toEqual([
      expect.objectContaining({
        _id: "the_quick_brown_fox",
        _deleted: true,
      }),
    ]);
    expect(deleteDocSpy).toHaveBeenCalledWith(
      expect.objectContaining({ id: "the_quick_brown_fox" })
    );
    await expect(db.get("the_quick_brown_fox")).rejects.toMatchObject({
      name: "not_found",
      reason: "deleted",
    });
  });

  it("only deletes the one document", async () => {
    await db.put({ _id: "id1", data: {}, meta: { humanId: "abc" } });
    await db.put({ _id: "id2", data: {}, meta: { humanId: "def" } });

    await deleteCmd({ db: dbName, quickId: "abc" });
    await expect(db.get("id1")).rejects.toMatchObject({
      name: "not_found",
      reason: "deleted",
    });
    expect(await db.get("id2")).toHaveProperty("_id");
  });

  it("calls quickId and deleteDoc", async () => {
    deleteDocSpy.mockReturnValue(
      Promise.resolve({ _id: "id", _rev: "abcdf", _deleted: true })
    );
    const quickIdsSpy = jest
      .spyOn(quickId, "quickIds")
      .mockImplementation(async (db, id) => [`${id}_to_delete`]);

    for (const quick of ["a", "part:lksdf", "1234", "__-sdfsdf"]) {
      await deleteCmd({ db: dbName, quickId: quick });
      expect(quickIdsSpy).toHaveBeenCalledWith(expect.anything(), quick);
      expect(deleteDocSpy).toHaveBeenCalledWith(
        expect.objectContaining({ id: quick + "_to_delete" })
      );
      quickIdsSpy.mockClear();
      deleteDocSpy.mockClear();
    }
  });

  it("outputs a DELETED message when show is standard", async () => {
    const originalLog = console.log;
    const mockLog = jest.fn();
    console.log = mockLog;

    await db.put({
      _id: "show_me_a_message",
      data: {},
      meta: { humanId: "somethingElse" },
    });
    await deleteCmd({ db: dbName, quickId: "show_me", show: Show.Standard });

    expect(mockLog).toHaveBeenCalledWith(expect.stringContaining("DELETE"));

    console.log = originalLog;
  });

  it("can delete multiple documents using a compound quickid", async () => {
    await db.put({ _id: "id1", data: {}, meta: { humanId: "abc" } });
    await db.put({ _id: "id2", data: {}, meta: { humanId: "def" } });
    await db.put({ _id: "id3", data: {}, meta: { humanId: "ghi" } });

    const returned = await deleteCmd({
      db: dbName,
      quickId: "abc,def,ghi,",
    });

    expect(returned).toHaveLength(3);
    expect(returned).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ _id: "id1", _deleted: true }),
        expect.objectContaining({ _id: "id2", _deleted: true }),
        expect.objectContaining({ _id: "id3", _deleted: true }),
      ])
    );

    await expect(db.get("id1")).rejects.toMatchObject({
      name: "not_found",
      reason: "deleted",
    });
    expect(deleteDocSpy).toHaveBeenCalledTimes(3);
  });
});
