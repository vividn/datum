import {
  mockedLogLifecycle,
  setNow,
  testDbLifecycle,
} from "../../__test__/test-utils";
import { addCmd } from "../../commands/addCmd";
import { DateTime, Duration, Settings } from "luxon";
import { occurCmd } from "../../commands/occurCmd";
import { Show } from "../../input/outputArgs";
import { setupCmd } from "../../commands/setupCmd";

// TODO: Make undo system more robust and more tested

describe("addCmd undo", () => {
  const { mockedLog } = mockedLogLifecycle();
  const dbName = "undo_addcmd_test";
  const db = testDbLifecycle(dbName);

  const mockNow = DateTime.utc(2020, 5, 10, 15, 25, 30);
  beforeEach(() => {
    setNow(mockNow.toString());
  });

  it("can undo adding documents with a known id", async () => {
    await addCmd("--id this_one_should_be_deleted");
    await addCmd("--id kept");

    await db.info().then((info) => {
      expect(info.doc_count).toEqual(2);
    });
    await db.get("this_one_should_be_deleted");
    await db.get("kept");

    mockedLog.mockReset();
    await addCmd("--id this_one_should_be_deleted --undo", {
      show: Show.Standard,
    });

    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("DELETE"));
    await db.info().then((info) => {
      expect(info.doc_count).toEqual(1);
    });
    await db.get("kept");
    await expect(db.get("this_one_should_be_deleted")).rejects.toMatchObject({
      name: "not_found",
      reason: "deleted",
    });
  });

  it("undoes a document with a time in the past if it contains occurTime", async () => {
    await setupCmd("");
    const now = "2021-06-28T06:30:00.000Z";
    const inAMinute = "2021-06-28T06:31:00.000Z";
    const insertedDoc = await occurCmd(`event -t ${now}`);
    const expectedId = `event:${now}`;
    expect(insertedDoc._id).toEqual(expectedId);

    await occurCmd(`event -t ${inAMinute} --undo`);
    await expect(db.get(expectedId)).rejects.toMatchObject({
      name: "not_found",
      reason: "deleted",
    });
  });

  it("prevents undo if created more than 15 minutes ago", async () => {
    const oldTime = mockNow.minus(
      Duration.fromObject({ minutes: 15, seconds: 30 })
    );

    await db.put({
      _id: "oldDoc",
      data: {},
      meta: { createTime: oldTime.toString() },
    });

    await expect(addCmd("--id oldDoc -u")).rejects.toThrowError(
      "Doc created more than fifteen minutes ago"
    );

    Settings.resetCaches();
  });

  it("allows undo if forceUndo", async () => {
    const docName = "oldDoc2";
    const oldTime = mockNow.minus(
      Duration.fromObject({ minutes: 15, seconds: 30 })
    );

    await db.put({
      _id: docName,
      data: {},
      meta: { createTime: oldTime.toString() },
    });
    await addCmd(` --id ${docName} -U`);
    await expect(db.get(docName)).rejects.toMatchObject({
      name: "not_found",
      reason: "deleted",
    });

    Settings.resetCaches();
  });

  it("allows undo if both forceUndo and undo", async () => {
    const docName = "oldDoc3";
    const oldTime = mockNow.minus(
      Duration.fromObject({ minutes: 15, seconds: 30 })
    );

    await db.put({
      _id: docName,
      data: {},
      meta: { createTime: oldTime.toString() },
    });
    await addCmd(` --id ${docName} -U`);
    await expect(db.get(docName)).rejects.toMatchObject({
      name: "not_found",
      reason: "deleted",
    });

    Settings.resetCaches();
  });
});
