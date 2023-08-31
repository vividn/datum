import { restoreNow, setNow, testDbLifecycle } from "../../test-utils";
import { DatumDocument } from "../../documentControl/DatumDocument";
import { occurCmd } from "../occurCmd";

describe("occurCmd", () => {
  const dbName = "occur_cmd_test";
  const db = testDbLifecycle(dbName);
  const now = "2023-08-05T16:00:00.000Z"

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

  it.todo(
    "does not interpret the first argument after field as duration if args.moment is true"
  );
  it.todo("with no-timestamp argument it is equalivalent to an addCmd");
  it.todo(
    "interprets a duration of 'start' or 'end' as a start or end command"
  );

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
