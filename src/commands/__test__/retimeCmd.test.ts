import { setNow, testDbLifecycle } from "../../__test__/test-utils";
import { setupCmd } from "../setupCmd";
import * as updateDoc from "../../documentControl/updateDoc";
import {
  DatumDocument,
  EitherDocument,
} from "../../documentControl/DatumDocument";
import { retimeCmd } from "../retimeCmd";
import * as quickId from "../../ids/quickId";
import { mock } from "jest-mock-extended";
import { Show } from "../../input/outputArgs";
import { addCmd } from "../addCmd";
import { LastDocsTooOldError } from "../../errors";
import { handleTimeArgs } from "../../input/timeArgs";
import { toDatumTime } from "../../time/datumTime";
import { now } from "../../time/timeUtils";
import { occurCmd } from "../occurCmd";

describe("retimeCmd", () => {
  const dbName = "retime_cmd_test";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    await setupCmd("");
  });

  it("can change the occurTime of a document using quickId", async () => {
    setNow("2024-05-10, 15:40");
    const originalDoc = await addCmd("field foo=bar");
    const originalOccurTime = (originalDoc as DatumDocument).data.occurTime;

    // Set a different time
    setNow("2024-05-10, 16:40");
    const retDocs = await retimeCmd([originalDoc._id]);

    expect(retDocs).toHaveLength(1);
    const updatedDoc = retDocs[0] as DatumDocument;
    expect(updatedDoc.data.occurTime).not.toEqual(originalOccurTime);
    expect(updatedDoc.data.occurTime).toEqual(toDatumTime(now()));

    // Verify database was updated
    const dbDoc = await db.get(updatedDoc._id);
    expect(dbDoc).toEqual(updatedDoc);
  });

  it("can use the quick time option to set time to 5 minutes ago", async () => {
    setNow("2024-05-10, 15:40");
    const { _id } = await addCmd("field foo=bar");

    // Quick option sets time to 5 minutes ago
    const retDocs = await retimeCmd(`${_id} -q`);

    expect(retDocs).toHaveLength(1);
    const updatedDoc = retDocs[0] as DatumDocument;

    // Check that the time is 5 minutes before the current time
    const { time: expectedTime } = handleTimeArgs({ quick: 1 });
    expect(updatedDoc.data.occurTime).toEqual(expectedTime);
  });

  it("can use other time options like --date and --time", async () => {
    const { _id } = await addCmd("field foo=bar");

    // Set a specific date and time
    const retDocs = await retimeCmd(`${_id} --date 2024-05-01 --time 10:30`);

    expect(retDocs).toHaveLength(1);
    const updatedDoc = retDocs[0] as DatumDocument;

    // Check that the time matches what we set
    const { time: expectedTime } = handleTimeArgs({
      date: "2024-05-01",
      time: "10:30",
    });
    expect(updatedDoc.data.occurTime).toEqual(expectedTime);
  });

  it("calls quickId and updateDoc with the right parameters", async () => {
    const updateDocReturn = mock<EitherDocument>();
    const quickIdSpy = jest
      .spyOn(quickId, "quickId")
      .mockImplementation(async () => ["quick_id"]);
    const updateDocSpy = jest
      .spyOn(updateDoc, "updateDoc")
      .mockReturnValue(Promise.resolve(updateDocReturn));

    setNow("2024-05-10, 15:40");
    const { time: expectedTime } = handleTimeArgs({});

    const retDocs = await retimeCmd("input_quick");

    expect(retDocs).toHaveLength(1);
    expect(retDocs[0]).toBe(updateDocReturn);
    expect(quickIdSpy).toHaveBeenCalledWith("input_quick", expect.anything());
    expect(updateDocSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        id: "quick_id",
        updateStrategy: "update",
        payload: { occurTime: expectedTime },
      }),
    );
  });

  it("outputs an UPDATE message when show is standard", async () => {
    const originalLog = console.log;
    const mockLog = jest.fn();
    console.log = mockLog;

    const { _id } = await addCmd("field foo=bar");
    await retimeCmd(_id, { show: Show.Standard });

    expect(mockLog).toHaveBeenCalledWith(expect.stringContaining("UPDATE"));

    console.log = originalLog;
  });

  it("can retime multiple documents with a compound quickId", async () => {
    const doc1 = await addCmd("field1 foo=bar");
    const doc2 = await addCmd("field2 bar=foo");

    setNow("2024-05-10, 16:40");
    const expectedTime = toDatumTime("2024-05-10T16:40:00");

    const returnValue = await retimeCmd(`,${doc1._id},${doc2._id}`);

    expect(returnValue).toHaveLength(2);
    expect((returnValue[0] as DatumDocument).data.occurTime).toEqual(
      expectedTime,
    );
    expect((returnValue[1] as DatumDocument).data.occurTime).toEqual(
      expectedTime,
    );

    // Verify database was updated
    const dbDoc1 = await db.get(doc1._id);
    const dbDoc2 = await db.get(doc2._id);
    expect((dbDoc1 as DatumDocument).data.occurTime).toEqual(expectedTime);
    expect((dbDoc2 as DatumDocument).data.occurTime).toEqual(expectedTime);
  });

  it("won't update the last doc if the ref is more than 15 minutes old", async () => {
    setNow("2024-05-10, 15:40");
    await addCmd("field foo=bar");
    setNow("+16");
    await expect(retimeCmd("")).rejects.toThrow(LastDocsTooOldError);
  });

  it("can remove the occur time if --omit-timestamp is requested", async () => {
    const { _id } = await occurCmd("field foo=bar");
    const returnValue = await retimeCmd([_id, "--omitTimestamp"]);

    expect(returnValue).toHaveLength(1);
    const updatedDoc = returnValue[0] as DatumDocument;
    expect(updatedDoc.data.occurTime).toBeUndefined();
  });
});
