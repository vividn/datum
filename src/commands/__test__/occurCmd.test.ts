import { restoreNow, setNow, testDbLifecycle } from "../../test-utils";
import { DatumDocument } from "../../documentControl/DatumDocument";
import { occurCmd } from "../occurCmd";
import { BadDurationError } from "../../errors";
import { setupCmd } from "../setupCmd";
import * as endCmdModule from "../endCmd";
import * as startCmdModule from "../startCmd";
import { getActiveState } from "../../state/getActiveState";
import { switchCmd } from "../switchCmd";

describe("occurCmd", () => {
  const dbName = "occur_cmd_test";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    await setupCmd({});
  });
  afterEach(() => {
    restoreNow();
  });

  it("creates a document with an occurTime", async () => {
    const now = "2023-08-05T16:00:00.000Z";
    setNow(now);

    const docCountBefore = (await db.info()).doc_count;
    const doc = await occurCmd({ field: "field" });

    await db.info().then((info) => {
      expect(info.doc_count).toEqual(docCountBefore + 1);
    });
    expect(doc.data.field).toEqual("field");
    expect(doc.data.occurTime.utc).toEqual(now);

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
      }),
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
    expect(newDoc.data).toHaveProperty(
      "occurTime.utc",
      "2021-08-23T12:00:00.000Z",
    );
    expect(newDoc.meta).not.toHaveProperty("occurTime");
  });

  it("stores the occurTime in DataOnly docs", async () => {
    const newDoc = (await occurCmd({
      field: "event",
      noMetadata: true,
      date: "2021-08-23",
      time: "12",
      timezone: "0",
    })) as DatumDocument;
    expect(newDoc).toHaveProperty("occurTime.utc", "2021-08-23T12:00:00.000Z");
    expect(newDoc).toHaveProperty("occurTime.o", 0);
  });

  it("stores offset", async () => {
    const newDoc = (await occurCmd({
      field: "event",
      date: "2021-08-23",
      time: "12",
      timezone: "+3",
    })) as DatumDocument;
    expect(newDoc.data).toMatchObject({
      occurTime: {
        o: 3,
        tz: "UTC+3",
      },
    });
  });

  it("stores offset from an IANA timezone", async () => {
    const newDoc = (await occurCmd({
      field: "event",
      date: "2023-12-04",
      time: "12",
      timezone: "Europe/Berlin",
    })) as DatumDocument;
    expect(newDoc.data).toMatchObject({
      occurTime: {
        o: 1,
        tz: "Europe/Berlin",
      },
    });
  });

  it("records the lastState if the active state is not false", async () => {
    expect(await getActiveState(db, "event")).toBe(null);
    const newDoc = (await occurCmd({
      field: "event",
    })) as DatumDocument;
    expect(newDoc.data).toMatchObject({
      lastState: null,
    });
    // Even 1 occur document is enough to switch active state to false from null
    expect(await getActiveState(db, "event")).toBe(false);

    const newDoc2 = (await occurCmd({
      field: "event",
    })) as DatumDocument;
    expect([false, undefined]).toContainEqual(newDoc2.data.lastState);

    await switchCmd({ field: "event", state: "happening" });
    const newDoc3 = (await occurCmd({
      field: "event",
    })) as DatumDocument;
    expect(newDoc3.data).toMatchObject({
      lastState: "happening",
    });
  });
});
