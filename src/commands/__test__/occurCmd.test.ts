import { restoreNow, setNow, testDbLifecycle } from "../../test-utils";
import { DatumDocument } from "../../documentControl/DatumDocument";
import { occurCmd } from "../occurCmd";
import { BadDurationError } from "../../errors";
import { setupCmd } from "../setupCmd";
import * as endCmdModule from "../endCmd";
import * as startCmdModule from "../startCmd";

describe("occurCmd", () => {
  const dbName = "occur_cmd_test";
  const db = testDbLifecycle(dbName);
  const now = "2023-08-05T16:00:00.000Z";

  beforeAll(() => {
    setNow(now);
  });
  afterAll(() => {
    restoreNow();
  });

  it("creates a document with an occurTime", async () => {
    const doc = await occurCmd({ field: "field" });

    await db.info().then((info) => {
      expect(info.doc_count).toEqual(1);
    });
    expect(doc.data.field).toEqual("field");
    expect(doc.data.occurTime).toEqual(now);

    expect(doc._id).toEqual(`field:${now}`);
    const dbDoc = await db.get(doc._id);
    expect(dbDoc).toEqual(doc);
  });

  it("interprets the first argument after field as duration if args.moment is false or undefined", async () => {
    // first argument is automatically assigned ot duration in the arguments. TODO: do a text only call to the cmd after that is supported
    const doc = await occurCmd({
      field: "field",
      moment: false,
      optional: "optional",
      duration: "30",
    });
    const doc2 = await occurCmd({
      field: "field",
      optional: "optional",
      duration: "30",
    });
    expect(doc.data).toMatchObject({ field: "field", dur: "PT30M" });
    expect(doc2.data).toMatchObject({ field: "field", dur: "PT30M" });
    expect(doc.data).not.toHaveProperty("optional");
    expect(doc2.data).not.toHaveProperty("optional");
  });

  it("does not interpret the first argument after field as duration if args.moment is true", async () => {
    const doc = await occurCmd({
      field: "field",
      moment: true,
      optional: "optional",
      duration: "30",
    });
    expect(doc.data).toMatchObject({ field: "field", optional: 30 });
    expect(doc.data).not.toHaveProperty("dur");
  });

  it('can skip the duration if the duration is given as "" or .', async () => {
    // TODO: rewrite this test as a string based call;
    const doc = await occurCmd({
      field: "field",
      moment: false,
      optional: "optional",
      duration: ".",
      data: [50],
    });
    const doc2 = await occurCmd({
      field: "field",
      moment: false,
      optional: "optional",
      duration: "",
      data: [50],
    });
    expect(doc.data).toMatchObject({ field: "field", optional: 50 });
    expect(doc.data).not.toHaveProperty("dur");
    expect(doc2.data).toMatchObject({ field: "field", optional: 50 });
    expect(doc2.data).not.toHaveProperty("dur");
  });

  // TODO: Make inferType throw errors on bad times,dates,durations
  it.skip("throws an error if the duration supplied is invalid", async () => {
    await expect(
      occurCmd({
        field: "field",
        optional: "optional",
        duration: "30asd",
      })
    ).rejects.toThrow(BadDurationError);
  });

  it("will not record an occurTime or duration if the no-timestamp argument is given", async () => {
    const doc = await occurCmd({
      field: "field",
      optional: "optional",
      duration: 30,
      noTimestamp: true,
    });
    expect(doc.data).not.toHaveProperty("occurTime");
    expect(doc.data).not.toHaveProperty("dur");
    expect(doc.data).toMatchObject({
      field: "field",
      optional: 30,
    });
  });

  it("interprets a duration of 'start' a start command", async () => {
    await setupCmd({ db: dbName });
    const startCmdSpy = jest.spyOn(startCmdModule, "startCmd");
    const startDoc = await occurCmd({
      field: "field",
      optional: "optional",
      duration: "start",
    });
    expect(startDoc.data).toMatchObject({ field: "field", state: true });
    expect(startDoc.data).not.toHaveProperty("dur");
    expect(startCmdSpy).toHaveBeenCalled();
  });

  it("interprets a duration of 'end' an end command", async () => {
    await setupCmd({ db: dbName });
    const endCmdSpy = jest.spyOn(endCmdModule, "endCmd");
    const endDoc = await occurCmd({
      field: "field",
      optional: "optional",
      duration: "end",
    });
    expect(endDoc.data).toMatchObject({ field: "field", state: false });
    expect(endDoc.data).not.toHaveProperty("dur");
    expect(endCmdSpy).toHaveBeenCalled();
  });

  it("stores the occurTime in the data", async () => {
    const newDoc = (await occurCmd({
      field: "event",
      date: "2021-08-23",
      time: "12",
      timezone: "0",
    })) as DatumDocument;
    expect(newDoc.data).toHaveProperty("occurTime", "2021-08-23T12:00:00.000Z");
    expect(newDoc.meta).not.toHaveProperty("occurTime");
  });

  it("stores the occurTime and utcOffset in DataOnly docs", async () => {
    const newDoc = (await occurCmd({
      field: "event",
      noMetadata: true,
      date: "2021-08-23",
      time: "12",
      timezone: "0",
    })) as DatumDocument;
    expect(newDoc).toHaveProperty("occurTime", "2021-08-23T12:00:00.000Z");
    expect(newDoc).toHaveProperty("occurUtcOffset", 0);
  });

  it("stores utcOffset", async () => {
    const newDoc = (await occurCmd({
      field: "event",
      date: "2021-08-23",
      time: "12",
      timezone: "0",
    })) as DatumDocument;
    expect(newDoc.data).toHaveProperty("occurUtcOffset", 0);
  });
});
