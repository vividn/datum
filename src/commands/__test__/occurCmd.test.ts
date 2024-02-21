import {
  deterministicHumanIds,
  restoreNow,
  setNow,
  testDbLifecycle,
} from "../../__test__/test-utils";
import { DatumDocument } from "../../documentControl/DatumDocument";
import { occurCmd } from "../occurCmd";
import { setupCmd } from "../setupCmd";
import { getActiveState } from "../../state/getActiveState";

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
    const doc = await occurCmd("field");

    await db.info().then((info) => {
      expect(info.doc_count).toEqual(docCountBefore + 1);
    });
    expect(doc.data.field).toEqual("field");
    expect(doc.data.occurTime.utc).toEqual(now);

    expect(doc._id).toEqual(`field:${now}`);
    const dbDoc = await db.get(doc._id);
    expect(dbDoc).toEqual(doc);
  });

  it("stores the occurTime in the data", async () => {
    const newDoc = (await occurCmd(
      "event -d 2021-08-23 -t 12 -z 0",
    )) as DatumDocument;
    expect(newDoc.data).toHaveProperty(
      "occurTime.utc",
      "2021-08-23T12:00:00.000Z",
    );
    expect(newDoc.meta).not.toHaveProperty("occurTime");
  });

  it("stores the occurTime in DataOnly docs", async () => {
    const newDoc = (await occurCmd(
      "event -m -d 2021-08-23 -t 12 -z 0",
    )) as DatumDocument;
    expect(newDoc).toHaveProperty("occurTime.utc", "2021-08-23T12:00:00.000Z");
    expect(newDoc).toHaveProperty("occurTime.o", 0);
  });

  it("stores offset", async () => {
    const newDoc = (await occurCmd(
      "event -d 2021-08-23 -t 12 -z +3",
    )) as DatumDocument;
    expect(newDoc.data).toMatchObject({
      occurTime: {
        o: 3,
        tz: "UTC+3",
      },
    });
  });

  it("stores offset from an IANA timezone", async () => {
    const newDoc = (await occurCmd(
      "event -d 2023-12-04 -t 12 -z Europe/Berlin",
    )) as DatumDocument;
    expect(newDoc.data).toMatchObject({
      occurTime: {
        o: 1,
        tz: "Europe/Berlin",
      },
    });
  });

  it("records lastState as null if the active state is null", async () => {
    expect(await getActiveState(db, "event")).toBe(null);
    const newDoc = await occurCmd("event");
    expect(newDoc.data).toMatchObject({
      lastState: null,
    });
    // Even 1 occur document is enough to switch active state to false from null
    expect(await getActiveState(db, "event")).toBe(false);

    const newDoc2 = await occurCmd("event");
    expect([false, undefined]).toContainEqual(newDoc2.data.lastState);
  });

  describe("change command", () => {
    deterministicHumanIds();

    beforeEach(async () => {
      setNow("2023-12-21 14:00");
    });
    afterAll(() => {
      restoreNow();
    });

    it("can become a start command by having start as a trailing word", async () => {
      expect(
        await occurCmd("field -K req1 -k opt1 start 30min"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become an end command by having start as a trailing word", async () => {
      expect(await occurCmd("field -K req1 -k opt1 end 30min")).toMatchSnapshot(
        {
          _rev: expect.any(String),
        },
      );
    });

    it("can become a switch command by having start as a trailing word", async () => {
      expect(
        await occurCmd("field -K req1 -k opt1 switch stateName 5m30s"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });
  });
});
