import {
  coloredChalk,
  deterministicHumanIds,
  mockedLogLifecycle,
  mockSpecs,
  setNow,
  testDbLifecycle,
} from "../../__test__/test-utils";
import { endCmd } from "../endCmd";
import { startCmd } from "../startCmd";
import { switchCmd } from "../switchCmd";
import { occurCmd } from "../occurCmd";
import { Show } from "../../input/outputArgs";
import { setupCmd } from "../setupCmd";
import { addCmd } from "../addCmd";
import { updateCmd } from "../updateCmd";
import { DateTime, Settings } from "luxon";
import { headCmd } from "../headCmd";
import { generateSampleMorning } from "../../__test__/generateSampleMorning";
import { datumTimeToLuxon } from "../../time/datumTime";
import { DatumDocument } from "../../documentControl/DatumDocument";

const yesterday = "2023-10-31";
const today = "2023-11-01";
const tomorrow = "2023-11-02";

describe("headCmd", () => {
  const { mockedLog } = mockedLogLifecycle();
  deterministicHumanIds();
  coloredChalk();
  const dbName = "head_cmd_test";
  testDbLifecycle(dbName);
  mockSpecs();

  beforeEach(async () => {
    await setupCmd("");
    setNow(`20:00 ${today}`);
  });

  it.todo("calls tailCmd with head: true and other arguments passed through");

  it("displays by default the first 10 occurrences in the database", async () => {
    await generateSampleMorning(today);
    const docs = await headCmd("", { show: Show.Standard });
    expect(docs.length).toBe(10);
    expect(docs[0]._id).toMatchInlineSnapshot(
      `"sleep:2023-10-31T23:20:00.000Z"`,
    );
    expect(docs.at(-1)?._id).toMatchInlineSnapshot(
      `"stretch:2023-11-01T09:37:00.000Z"`,
    );
    expect(mockedLog.mock.calls).toMatchSnapshot();
  });

  it("will display all if there are less than 10 occurrences in db", async () => {
    setNow(`8am ${today}`);
    await occurCmd("caffeine amount=100");
    setNow(`10am`);
    await switchCmd("project household");
    setNow(`10:30`);
    await endCmd("project");

    const docs = await headCmd("", { show: Show.Standard });
    expect(docs.length).toBe(3);
    expect(mockedLog.mock.calls).toMatchSnapshot();
  });

  it("can display the first n occurrences", async () => {
    await generateSampleMorning(today);
    const docs = await headCmd("-n 5");
    expect(docs.length).toBe(5);
  });

  it("will display all if there are less than n occurrences in db", async () => {
    setNow(`8am ${today}`);
    await occurCmd("caffeine amount=100");
    setNow(`10am`);
    await switchCmd("project household");
    setNow(`10:30`);
    await endCmd("project");

    const docs = await headCmd("-n 5");
    expect(docs.length).toBe(3);
  });

  it("displays first occurrences of a specific field", async () => {
    await generateSampleMorning(today);
    const docs = await headCmd("stretch");
    expect(docs.length).toBe(4);
    await generateSampleMorning(yesterday);
    const docs2 = (await headCmd("stretch")) as DatumDocument[];
    expect(docs2.length).toBe(8);
    expect(docs2.map((doc) => doc.data.field as string)).toEqual(
      Array(8).fill("stretch"),
    );
  });

  describe("first metrics", () => {
    let occur1Id: string, occur2Id: string, occur3Id: string, addId: string;
    beforeEach(async () => {
      setNow("19:00");
      ({ _id: occur1Id } = await occurCmd("alcohol"));
      setNow("20:00");
      ({ _id: addId } = await addCmd(
        "person name='john doe' age=35 --id %name",
      ));
      setNow("21:00");
      ({ _id: occur3Id } = await endCmd("socialize"));
      setNow("+1");
      ({ _id: occur2Id } = await startCmd("socialize -t 19:30"));

      // modify the doc without an occur time
      setNow("22:00");
      await updateCmd(`"${addId}" age=36`);
    });

    it("defaults to a hybrid display of occurTime and createTime", async () => {
      const docs = await headCmd("", { show: Show.Standard });
      expect(docs.length).toBe(4);
      expect(docs.map((doc) => doc._id)).toEqual([
        occur1Id,
        occur2Id,
        addId,
        occur3Id,
      ]);
      expect(mockedLog.mock.calls).toMatchSnapshot();

      const explicitHybridDocs = await headCmd("-m hybrid");
      expect(explicitHybridDocs).toEqual(docs);
    });

    it("can just display occurTime head", async () => {
      const docs = await headCmd("-m occur");
      expect(docs.length).toBe(3);
      expect(docs.map((doc) => doc._id)).toEqual([
        occur1Id,
        occur2Id,
        occur3Id,
      ]);
      expect(mockedLog.mock.calls).toMatchSnapshot();
    });

    it("can display modifyTime head", async () => {
      const docs = await headCmd("-m modify");
      expect(docs.length).toBe(4);
      expect(docs.map((doc) => doc._id)).toEqual([
        occur1Id,
        occur3Id,
        occur2Id,
        addId,
      ]);
      expect(mockedLog.mock.calls).toMatchSnapshot();
    });

    it("can display createTime head", async () => {
      const docs = await headCmd("-m create");
      expect(docs.length).toBe(4);
      expect(docs.map((doc) => doc._id)).toEqual([
        occur1Id,
        addId,
        occur3Id,
        occur2Id,
      ]);
    });
  });

  it("can display a head from a certain moment in time", async () => {
    await generateSampleMorning(today);
    const docs1 = await headCmd("-t 10:15");
    expect(docs1.length).toBe(8);
    expect(docs1.at(-1)?._id).toMatchInlineSnapshot(
      `"project:2023-11-01T11:45:00.000Z"`,
    );

    const docs2 = await headCmd(`-d today -t 11`);
    expect(docs2.length).toBe(4);

    const docs3 = await headCmd("-d +1 -t 7");
    expect(docs3.length).toBe(0);
  });

  it("displays all occurrences on a day if date is given without time", async () => {
    await generateSampleMorning(today);
    await generateSampleMorning(yesterday);
    const docs = (await headCmd(`-d ${today}`)) as DatumDocument[];
    const yesterdayDocs = (await headCmd("-y")) as DatumDocument[];

    expect(docs.length).toBeGreaterThan(10);
    expect(
      docs
        .map((doc) => doc.data.occurTime?.utc)
        .every(
          (occurTime) =>
            DateTime.fromISO(occurTime ?? "").toISODate() === today,
        ),
    ).toBe(true);
    expect(
      yesterdayDocs
        .map((doc) => doc.data.occurTime?.utc)
        .every(
          (occurTime) =>
            DateTime.fromISO(occurTime ?? "").toISODate() === yesterday,
        ),
    ).toEqual(true);
  });

  it("displays occurrences on a given day properly even when user is in a different timezone", async () => {
    // use New Zealand so that early events in the sample morning are previous day in UTC
    Settings.defaultZone = "Pacific/Auckland";
    setNow(`${today} 20:00`);
    await generateSampleMorning(today);
    await generateSampleMorning(tomorrow);
    const docs = (await headCmd(`-d today`)) as DatumDocument[];
    expect(docs.length).toBeGreaterThan(10);

    // when mapping just from utc, not all should be on today
    const utcDates = docs.map((doc) => {
      const utc = doc.data.occurTime?.utc;
      if (!utc) {
        throw Error("bad occurTime");
      }
      return DateTime.fromISO(utc).toUTC().toISODate();
    });
    expect(utcDates.every((date) => date === today)).toBe(false);

    const localDates = docs.map((doc) =>
      datumTimeToLuxon(doc.data.occurTime)?.toISODate(),
    );
    expect(localDates.every((date) => date === today)).toBe(true);

    Settings.defaultZone = "system";
  });

  it("when requesting the entire date, have full day occurrences be at the top", async () => {
    Settings.defaultZone = "Pacific/Auckland";
    setNow(`${today} 20:00`);
    await generateSampleMorning(today);
    await generateSampleMorning(tomorrow);
    const lengthWithoutFullDayDocs = (await headCmd("-d +1")).length;

    const fullDayDoc1 = await occurCmd("field -d +1");
    const fullDayDoc2 = await occurCmd(`otherField -d ${tomorrow}`);
    const docs = await headCmd("-d +1");
    expect(docs.length).toEqual(lengthWithoutFullDayDocs + 2);

    expect(docs.slice(0, 2)).toEqual([fullDayDoc1, fullDayDoc2]);
    Settings.defaultZone = "system";
  });

  it("displays only first n occurrences on a day if date and -n is given", async () => {
    Settings.defaultZone = "Pacific/Auckland";
    setNow(`${today} 20:00`);
    await generateSampleMorning(today);
    await generateSampleMorning(tomorrow);

    const fullDayDoc1 = await occurCmd("field -d +1");
    const fullDayDoc2 = await occurCmd("otherField -d +1");
    const allDocs = await headCmd("-d +1");
    const limitedDocs = await headCmd("-d +1 -n 5");

    expect(limitedDocs.length).toEqual(5);
    expect(limitedDocs[0]).toEqual(fullDayDoc1);
    expect(limitedDocs[1]).toEqual(fullDayDoc2);
    expect(limitedDocs).toEqual(allDocs.slice(0, 5));
    Settings.defaultZone = "system";
  });

  it("can display a custom format for the head commands", async () => {
    await generateSampleMorning(today);
    await generateSampleMorning(tomorrow);

    await occurCmd("", { show: Show.None });
    await headCmd(" --format '::%field%::'");
    expect(mockedLog.mock.calls).toMatchInlineSnapshot(`
      [
        [
          "::sleep::
      ::sleep::
      ::project::
      ::text::
      ::project::
      ::text::
      ::environment::
      ::stretch::
      ::run::
      ::stretch::",
        ],
      ]
    `);
  });

  it("does not display anything when show is None", async () => {
    await generateSampleMorning(today);
    await headCmd("", { show: Show.None });
    expect(mockedLog).not.toHaveBeenCalled();
  });
});
