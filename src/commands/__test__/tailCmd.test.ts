import {
    delay,
  deterministicHumanIds,
  mockedLogLifecycle,
  setNow,
  testDbLifecycle,
} from "../../__test__/test-utils";
import { endCmd } from "../endCmd";
import { startCmd } from "../startCmd";
import { switchCmd } from "../switchCmd";
import { occurCmd } from "../occurCmd";
import { tailCmd } from "../tailCmd";
import { Show } from "../../input/outputArgs";
import { setupCmd } from "../setupCmd";
import { addCmd } from "../addCmd";
import { updateCmd } from "../updateCmd";
import { DateTime, Settings } from "luxon";
import { datumTimeToLuxon } from "../../time/timeUtils";
import { generateSampleMorning } from "../../__test__/generateSampleMorning";

const yesterday = "2023-10-15";
const today = "2023-10-16";
const tomorrow = "2023-10-17";

describe("tailCmd", () => {
  const { mockedLog } = mockedLogLifecycle();
  deterministicHumanIds();
  const dbName = "tail_cmd_test";
  testDbLifecycle(dbName);

  beforeEach(async () => {
    await setupCmd("");
    setNow(`20:00 ${today}`);
  });

  it("displays by default the last 10 occurrences in the database", async () => {
    await generateSampleMorning(today);
    const docs = await tailCmd("", { show: Show.Standard });
    expect(docs.length).toBe(10);
    expect(docs[0]._id).toMatchInlineSnapshot(
      `"environment:2023-10-16T09:30:00.000Z"`,
    );
    expect(docs.at(-1)?._id).toMatchInlineSnapshot(
      `"caffeine:2023-10-16T11:00:00.000Z"`,
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

    const docs = await tailCmd("", { show: Show.Standard });
    expect(docs.length).toBe(3);
    expect(mockedLog.mock.calls).toMatchSnapshot();
  });

  it("can display the last n occurrences", async () => {
    await generateSampleMorning(today);
    const docs = await tailCmd("-n 5");
    expect(docs.length).toBe(5);
  });

  it("will display all if there are less than n occurences in db", async () => {
    setNow(`8am ${today}`);
    await occurCmd("caffeine amount=100");
    setNow(`10am`);
    await switchCmd("project househould");
    setNow(`10:30`);
    await endCmd("project");

    const docs = await tailCmd("-n 5");
    expect(docs.length).toBe(3);
  });

  it("displays last occurrences of a specific field", async () => {
    await generateSampleMorning(today);
    const docs = await tailCmd("stretch");
    expect(docs.length).toBe(4);
    await generateSampleMorning(yesterday);
    const docs2 = await tailCmd("stretch");
    expect(docs2.length).toBe(8);
    expect(docs2.map((doc) => doc.data.field)).toEqual(
      Array(8).fill("stretch"),
    );
  });

  describe("tail metrics", () => {
    let occur1Id: string, occur2Id: string, occur3Id: string, addId: string;
    beforeEach(async () => {
      setNow("19:00");
      ({ _id: occur1Id } = await occurCmd("alcohol"));
      setNow("20:00");
      ({ _id: addId } = await addCmd(
        `person -b '{ name: "john doe", age: 35 }' --id %name`,
      ));
      setNow("21:00");
      ({ _id: occur3Id } = await endCmd("socialize"));
      setNow("+1");
      ({ _id: occur2Id } = await startCmd("socialize -t 1930"));

      // modify the doc without an occur time
      setNow("22:00");
      await updateCmd(`"${addId}" age=36`);
    });

    it("defaults to a hybrid display of occurTime and createTime", async () => {
      const docs = await tailCmd("", { show: Show.Standard });
      expect(docs.length).toBe(4);
      expect(docs.map((doc) => doc._id)).toEqual([
        occur1Id,
        occur2Id,
        addId,
        occur3Id,
      ]);
      expect(mockedLog.mock.calls).toMatchSnapshot();

      const explicitHybridDocs = await tailCmd("--metric hybrid");
      expect(explicitHybridDocs).toEqual(docs);
    });

    it("can just display occurTime tail", async () => {
      const docs = await tailCmd("-m occur");
      expect(docs.length).toBe(3);
      expect(docs.map((doc) => doc._id)).toEqual([
        occur1Id,
        occur2Id,
        occur3Id,
      ]);
      expect(mockedLog.mock.calls).toMatchSnapshot();
    });

    it("can display modifyTime tail", async () => {
      const docs = await tailCmd("-m modify");
      expect(docs.length).toBe(4);
      expect(docs.map((doc) => doc._id)).toEqual([
        occur1Id,
        occur3Id,
        occur2Id,
        addId,
      ]);
      expect(mockedLog.mock.calls).toMatchSnapshot();
    });

    it("can display createTime tail", async () => {
      const docs = await tailCmd("-m create");
      expect(docs.length).toBe(4);
      expect(docs.map((doc) => doc._id)).toEqual([
        occur1Id,
        addId,
        occur3Id,
        occur2Id,
      ]);
    });
  });

  it("can display a tail from a certain moment in time", async () => {
    await generateSampleMorning(today);
    const docs1 = await tailCmd("-t 9:30");
    expect(docs1.length).toBe(8);
    expect(docs1.at(-1)?._id).toMatchInlineSnapshot(
      `"stretch:2023-10-16T09:30:00.000Z"`,
    );

    const docs2 = await tailCmd("-d yesterday -t 23:30");
    expect(docs2.length).toBe(1);

    const docs3 = await tailCmd("-yy -t 22");
    expect(docs3.length).toBe(0);
  });

  it("displays future entries if no specific time is given or --omit-timestamp is given", async () => {
    setNow(`20:00 ${today}`);
    await generateSampleMorning(today);
    await generateSampleMorning(tomorrow);

    const docs = await tailCmd("");
    const lastOccur = docs.at(-1)?.data.occurTime.utc;
    expect(DateTime.fromISO(lastOccur).toISODate()).toEqual(tomorrow);

    const docsomitTimestamp = await tailCmd("");
    const lastOccuromitTimestamp = docsomitTimestamp.at(-1)?.data.occurTime.utc;
    expect(DateTime.fromISO(lastOccuromitTimestamp).toISODate()).toEqual(
      tomorrow,
    );
  });

  it("does not display future entries if now is given specifically as the time", async () => {
    setNow(`20:00 ${today}`);
    await generateSampleMorning(today);
    await generateSampleMorning(tomorrow);

    const docs = await tailCmd("-t now");
    const lastOccur = docs.at(-1)?.data.occurTime.utc;
    expect(DateTime.fromISO(lastOccur).toISODate()).toEqual(today);
  });

  it("displays all occurrences on a day if date is given without time", async () => {
    await generateSampleMorning(today);
    await generateSampleMorning(yesterday);
    const docs = await tailCmd("-d today");
    const yesterdayDocs = await tailCmd("-y");

    expect(docs.length).toBeGreaterThan(10);
    expect(
      docs
        .map((doc) => doc.data.occurTime.utc)
        .every(
          (occurTime) => DateTime.fromISO(occurTime).toISODate() === today,
        ),
    ).toBe(true);
    expect(
      yesterdayDocs
        .map((doc) => doc.data.occurTime.utc)
        .every(
          (occurTime) => DateTime.fromISO(occurTime).toISODate() === yesterday,
        ),
    ).toEqual(true);
  });

  it("displays occurrences on a given day properly even when user is in a different timezone", async () => {
    // use New Zealand so that early events in the sample morning are previous day in UTC
    Settings.defaultZone = "Pacific/Auckland";
    setNow(`${today} 20:00`);
    await generateSampleMorning(today);
    await generateSampleMorning(tomorrow);
    const docs = await tailCmd(`-d ${today}`);
    expect(docs.length).toBeGreaterThan(10);

    // when mapping just from utc, not all should be on today
    const utcDates = docs.map((doc) =>
      DateTime.fromISO(doc.data.occurTime.utc).toUTC().toISODate(),
    );
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
    const lengthWithoutFullDayDocs = (await tailCmd(`-d ${tomorrow}`)).length;

    const fullDayDoc1 = await occurCmd(`field -d ${tomorrow}`);
    const fullDayDoc2 = await occurCmd("otherField -d +1");
    const docs = await tailCmd(`-d ${tomorrow}`);
    expect(docs.length).toEqual(lengthWithoutFullDayDocs + 2);

    expect(docs.slice(0, 2)).toEqual([fullDayDoc1, fullDayDoc2]);
    Settings.defaultZone = "system";
  });

  it("displays only n latest occurrences on a day if date and -n is given", async () => {
    Settings.defaultZone = "Pacific/Auckland";
    setNow(`${today} 20:00`);
    await generateSampleMorning(today);
    await generateSampleMorning(tomorrow);

    const fullDayDoc1 = await occurCmd(`field -d ${tomorrow}`);
    const fullDayDoc2 = await occurCmd(`otherField -d +1`);
    const allDocs = await tailCmd(`-d ${tomorrow}`);
    const limitedDocs = await tailCmd(`-d ${tomorrow} -n 5`);

    expect(limitedDocs.length).toEqual(5);
    expect(limitedDocs).not.toContainEqual(fullDayDoc1);
    expect(limitedDocs).not.toContainEqual(fullDayDoc2);
    expect(limitedDocs).toEqual(allDocs.slice(-5));
    Settings.defaultZone = "system";
  });

  it("can display a custom format for the tail commands", async () => {
    await generateSampleMorning(today);
    await generateSampleMorning(tomorrow);

    await occurCmd("", { show: Show.None });
    await tailCmd("--format-string ::%field%::");
    expect(mockedLog.mock.calls).toMatchInlineSnapshot(`
      [
        [
          "::environment::
      ::stretch::
      ::run::
      ::stretch::
      ::run::
      ::stretch::
      ::environment::
      ::stretch::
      ::pushup::
      ::caffeine::",
        ],
      ]
    `);
  });

  it("does not display anything when show is None", async () => {
    await generateSampleMorning(today);
    await tailCmd("", { show: Show.None });
    expect(mockedLog).not.toHaveBeenCalled();
  });

  it("can watch tail output", async () => {
    setNow(`12:00 ${today}`);
    await generateSampleMorning(today);
    const _tailPromise = tailCmd("-w", { show: Show.Standard });
    await delay(100);
    expect(mockedLog).toHaveBeenCalledTimes(1);
    expect(mockedLog.mock.calls[0]).toMatchSnapshot();
    mockedLog.mockClear();
    await delay(100);
    expect(mockedLog).not.toHaveBeenCalled();
    await occurCmd("pushup amount=10");
    setNow(`12:30`);
    await occurCmd("caffeine amount=100 -c coffee");
    await delay(100);
    expect(mockedLog).toHaveBeenCalledTimes(2);
    expect(mockedLog.mock.calls).toMatchSnapshot();
  });
});
