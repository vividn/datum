import {
  deterministicHumanIds,
  mockedLogLifecycle,
  popNow,
  pushNow,
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

const yesterday = "2023-10-15";
const today = "2023-10-16";
const tomorrow = "2023-10-17";

export async function generateSampleMorning(date: string): Promise<void> {
  pushNow(`8:30 ${date}`);
  await endCmd({ field: "sleep" });
  setNow("+5");
  await startCmd({ field: "sleep", yesterday: 1, time: "23:20" });
  setNow("8:40");
  await switchCmd({ field: "project", state: "german" });
  await switchCmd({ field: "text", state: "fiction_book" });
  setNow("9:10");
  await endCmd({ field: "project" });
  await endCmd({ field: "text" });
  setNow("9:30");
  await switchCmd({ field: "environment", state: "outside" });
  await startCmd({ field: "stretch" });
  setNow("+7");
  await endCmd({ field: "stretch" });
  await startCmd({ field: "run" });
  setNow("+30");
  await endCmd({ field: "run", data: ["distance=5.4"] });
  await startCmd({ field: "stretch" });
  setNow("+8");
  await endCmd({ field: "stretch" });
  await switchCmd({ field: "environment", state: "home" });
  setNow("+3");
  await occurCmd({ field: "pushup", data: ["amount=10"] });
  setNow("11");
  await occurCmd({
    field: "caffeine",
    data: ["amount=100"],
    comment: "coffee",
  });
  popNow();
}

describe("tailCmd", () => {
  const mockedLog = mockedLogLifecycle();
  deterministicHumanIds();
  const dbName = "tail_cmd_test";
  testDbLifecycle(dbName);

  beforeEach(async () => {
    await setupCmd({});
    setNow(`20:00 ${today}`);
  });

  it("displays by default the last 10 occurrences in the database", async () => {
    await generateSampleMorning(today);
    const docs = await tailCmd({ show: Show.Standard });
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
    await occurCmd({ field: "caffeine", data: ["amount=100"] });
    setNow(`10am`);
    await switchCmd({ field: "project", state: "household" });
    setNow(`10:30`);
    await endCmd({ field: "project" });

    const docs = await tailCmd({ show: Show.Standard });
    expect(docs.length).toBe(3);
    expect(mockedLog.mock.calls).toMatchSnapshot();
  });

  it("can display the last n occurrences", async () => {
    await generateSampleMorning(today);
    const docs = await tailCmd({ n: 5 });
    expect(docs.length).toBe(5);
  });

  it("will display all if there are less than n occurences in db", async () => {
    setNow(`8am ${today}`);
    await occurCmd({ field: "caffeine", data: ["amount=100"] });
    setNow(`10am`);
    await switchCmd({ field: "project", state: "household" });
    setNow(`10:30`);
    await endCmd({ field: "project" });

    const docs = await tailCmd({ n: 5 });
    expect(docs.length).toBe(3);
  });

  it("displays last occurrences of a specific field", async () => {
    await generateSampleMorning(today);
    const docs = await tailCmd({ field: "stretch" });
    expect(docs.length).toBe(4);
    await generateSampleMorning(yesterday);
    const docs2 = await tailCmd({ field: "stretch" });
    expect(docs2.length).toBe(8);
    expect(docs2.map((doc) => doc.data.field)).toEqual(
      Array(8).fill("stretch"),
    );
  });

  describe("tail metrics", () => {
    let occur1Id: string, occur2Id: string, occur3Id: string, addId: string;
    beforeEach(async () => {
      setNow("19:00");
      ({ _id: occur1Id } = await occurCmd({ field: "alcohol" }));
      setNow("20:00");
      ({ _id: addId } = await addCmd({
        field: "person",
        baseData: { name: "john doe", age: 35 },
        idPart: "%name",
      }));
      setNow("21:00");
      ({ _id: occur3Id } = await endCmd({ field: "socialize" }));
      setNow("+1");
      ({ _id: occur2Id } = await startCmd({
        field: "socialize",
        time: "19:30",
      }));

      // modify the doc without an occur time
      setNow("22:00");
      await updateCmd({ quickId: addId, data: ["age=36"] });
    });

    it("defaults to a hybrid display of occurTime and createTime", async () => {
      const docs = await tailCmd({ show: Show.Standard });
      expect(docs.length).toBe(4);
      expect(docs.map((doc) => doc._id)).toEqual([
        occur1Id,
        occur2Id,
        addId,
        occur3Id,
      ]);
      expect(mockedLog.mock.calls).toMatchSnapshot();

      const explicitHybridDocs = await tailCmd({ metric: "hybrid" });
      expect(explicitHybridDocs).toEqual(docs);
    });

    it("can just display occurTime tail", async () => {
      const docs = await tailCmd({ metric: "occur" });
      expect(docs.length).toBe(3);
      expect(docs.map((doc) => doc._id)).toEqual([
        occur1Id,
        occur2Id,
        occur3Id,
      ]);
      expect(mockedLog.mock.calls).toMatchSnapshot();
    });

    it("can display modifyTime tail", async () => {
      const docs = await tailCmd({ metric: "modify" });
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
      const docs = await tailCmd({ metric: "create" });
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
    const docs1 = await tailCmd({ time: "9:30" });
    expect(docs1.length).toBe(8);
    expect(docs1.at(-1)?._id).toMatchInlineSnapshot(
      `"stretch:2023-10-16T09:30:00.000Z"`,
    );

    const docs2 = await tailCmd({ date: yesterday, time: "23:30" });
    expect(docs2.length).toBe(1);

    const docs3 = await tailCmd({ yesterday: 2, time: "22" });
    expect(docs3.length).toBe(0);
  });

  it("displays future entries if no specific time is given or --no-timestamp is given", async () => {
    setNow(`20:00 ${today}`);
    await generateSampleMorning(today);
    await generateSampleMorning(tomorrow);

    const docs = await tailCmd({});
    const lastOccur = docs.at(-1)?.data.occurTime.utc;
    expect(DateTime.fromISO(lastOccur).toISODate()).toEqual(tomorrow);

    const docsNoTimestamp = await tailCmd({});
    const lastOccurNoTimestamp = docsNoTimestamp.at(-1)?.data.occurTime.utc;
    expect(DateTime.fromISO(lastOccurNoTimestamp).toISODate()).toEqual(
      tomorrow,
    );
  });

  it("does not display future entries if now is given specifically as the time", async () => {
    setNow(`20:00 ${today}`);
    await generateSampleMorning(today);
    await generateSampleMorning(tomorrow);

    const docs = await tailCmd({ time: "now" });
    const lastOccur = docs.at(-1)?.data.occurTime.utc;
    expect(DateTime.fromISO(lastOccur).toISODate()).toEqual(today);
  });

  it("displays all occurrences on a day if date is given without time", async () => {
    await generateSampleMorning(today);
    await generateSampleMorning(yesterday);
    const docs = await tailCmd({ date: "today" });
    const yesterdayDocs = await tailCmd({ yesterday: 1 });

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
    await generateSampleMorning(today);
    await generateSampleMorning(tomorrow);
    const docs = await tailCmd({ date: today });
    expect(docs.length).toBeGreaterThan(10);

    // when mapping just from utc, not all should be on today
    const utcDates = docs.map((doc) =>
      DateTime.fromISO(doc.data.occurTime.utc).toUTC().toISODate(),
    );
    expect(utcDates.every((date) => date === today)).toBe(false);

    const localDates = docs.map(
      (doc) => datumTimeToLuxon(doc.data.occurTime)?.toISODate(),
    );
    expect(localDates.every((date) => date === today)).toBe(true);

    Settings.defaultZone = "system";
  });

  it("when requesting the entire date, have full day occurrences be at the top", async () => {
    Settings.defaultZone = "Pacific/Auckland";
    await generateSampleMorning(today);
    await generateSampleMorning(tomorrow);
    const lengthWithoutFullDayDocs = (await tailCmd({ date: tomorrow })).length;

    const fullDayDoc1 = await occurCmd({ field: "field", date: tomorrow });
    const fullDayDoc2 = await occurCmd({ field: "otherField", date: tomorrow });
    const docs = await tailCmd({ date: tomorrow });
    expect(docs.length).toEqual(lengthWithoutFullDayDocs + 2);

    expect(docs.slice(0, 2)).toEqual([fullDayDoc1, fullDayDoc2]);
    Settings.defaultZone = "system";
  });

  it("displays only n latest occurrences on a day if date and -n is given", async () => {
    Settings.defaultZone = "Pacific/Auckland";
    await generateSampleMorning(today);
    await generateSampleMorning(tomorrow);

    const fullDayDoc1 = await occurCmd({ field: "field", date: tomorrow });
    const fullDayDoc2 = await occurCmd({ field: "otherField", date: tomorrow });
    const allDocs = await tailCmd({ date: tomorrow });
    const limitedDocs = await tailCmd({ date: tomorrow, n: 5 });

    expect(limitedDocs.length).toEqual(5);
    expect(limitedDocs).not.toContainEqual(fullDayDoc1);
    expect(limitedDocs).not.toContainEqual(fullDayDoc2);
    expect(limitedDocs).toEqual(allDocs.slice(-5));
    Settings.defaultZone = "system";
  });

  it("can display a custom format for the tail commands", async () => {
    await generateSampleMorning(today);
    await generateSampleMorning(tomorrow);

    await occurCmd({ show: Show.None });
    await tailCmd({ formatString: "--%field%--" });
    expect(mockedLog.mock.calls).toMatchInlineSnapshot(`
      Array [
        Array [
          "--environment--",
        ],
        Array [
          "--stretch--",
        ],
        Array [
          "--run--",
        ],
        Array [
          "--stretch--",
        ],
        Array [
          "--run--",
        ],
        Array [
          "--stretch--",
        ],
        Array [
          "--environment--",
        ],
        Array [
          "--stretch--",
        ],
        Array [
          "--pushup--",
        ],
        Array [
          "--caffeine--",
        ],
      ]
    `);
  });

  it("does not display anything when show is None", async () => {
    await generateSampleMorning(today);
    await tailCmd({ show: Show.None });
    expect(mockedLog).not.toHaveBeenCalled();
  });
});
