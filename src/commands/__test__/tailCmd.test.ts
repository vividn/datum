import {
  coloredChalk,
  delay,
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
import { tailCmd } from "../tailCmd";
import { Show } from "../../input/outputArgs";
import { setupCmd } from "../setupCmd";
import { addCmd } from "../addCmd";
import { updateCmd } from "../updateCmd";
import { DateTime, Settings } from "luxon";
import { generateSampleMorning } from "../../__test__/generateSampleMorning";
import { datumTimeToLuxon } from "../../time/datumTime";
import { DatumDocument } from "../../documentControl/DatumDocument";

const yesterday = "2023-10-15";
const today = "2023-10-16";
const tomorrow = "2023-10-17";

describe("tailCmd", () => {
  const { mockedLog } = mockedLogLifecycle();
  deterministicHumanIds();
  coloredChalk();
  const dbName = "tail_cmd_test";
  testDbLifecycle(dbName);
  mockSpecs();

  beforeEach(async () => {
    await setupCmd("");
    setNow(`20:00 ${today}`);
  });

  it("displays by default the last 10 occurrences in the database", async () => {
    await generateSampleMorning(today);
    const docs = await tailCmd("", { show: Show.Standard });
    expect(docs.length).toBe(10);
    expect(docs[0]._id).toMatchInlineSnapshot(`"run:2023-10-16T10:07:00.000Z"`);
    expect(docs.at(-1)?._id).toMatchInlineSnapshot(
      `"project:2023-10-16T11:45:00.000Z"`,
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
    const docs = (await tailCmd("stretch")) as DatumDocument[];
    expect(docs.length).toBe(4);
    await generateSampleMorning(yesterday);
    const docs2 = (await tailCmd("stretch")) as DatumDocument[];
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

    const docs = (await tailCmd("")) as DatumDocument[];
    const lastOccur = docs.at(-1)?.data.occurTime?.utc;
    expect(DateTime.fromISO(lastOccur ?? "").toISODate()).toEqual(tomorrow);

    const docsomitTimestamp = (await tailCmd("")) as DatumDocument[];
    const lastOccuromitTimestamp =
      docsomitTimestamp.at(-1)?.data.occurTime?.utc;
    expect(DateTime.fromISO(lastOccuromitTimestamp ?? "").toISODate()).toEqual(
      tomorrow,
    );
  });

  it("does not display future entries if now is given specifically as the time", async () => {
    setNow(`20:00 ${today}`);
    await generateSampleMorning(today);
    await generateSampleMorning(tomorrow);

    const docs = (await tailCmd("-t now")) as DatumDocument[];
    const lastOccur = docs.at(-1)?.data.occurTime?.utc;
    expect(DateTime.fromISO(lastOccur ?? "").toISODate()).toEqual(today);
  });

  it("displays all occurrences on a day if date is given without time", async () => {
    await generateSampleMorning(today);
    await generateSampleMorning(yesterday);
    const docs = (await tailCmd("-d today")) as DatumDocument[];
    const yesterdayDocs = (await tailCmd("-y")) as DatumDocument[];

    expect(docs.length).toBeGreaterThan(10);
    expect(
      docs
        .map((doc) => doc.data.occurTime?.utc ?? "")
        .every(
          (occurTime) => DateTime.fromISO(occurTime).toISODate() === today,
        ),
    ).toBe(true);
    expect(
      yesterdayDocs
        .map((doc) => doc.data.occurTime?.utc ?? "")
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
    const docs = (await tailCmd(`-d ${today}`)) as DatumDocument[];
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
          "::run::
      ::stretch::
      ::environment::
      ::stretch::
      ::pushup::
      ::environment::
      ::caffeine::
      ::project::
      ::project::
      ::project::",
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
    tailCmd("-w", { show: Show.Standard });
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

    // Note: the tail watch is automatically completed when the database is destoryed in the afterEach
  });

  it("can display extra data columns in the output", async () => {
    await generateSampleMorning(today);
    await tailCmd("--column amount,distance --column comment", {
      show: Show.Standard,
    });
    expect(mockedLog).toHaveBeenCalledTimes(1);
    expect(mockedLog.mock.calls[0]).toMatchInlineSnapshot(`
      [
        "      time  field        state           dur  hid    amount  distance  comment 
      10:07:00[90m+0[39m  [48;2;165;49;8m[38;2;255;255;255mrun[39m[49m          [48;2;165;49;8m[38;2;255;255;255m [39m[49mend                 869r3          5.4               
      10:07:00[90m+0[39m  [48;2;126;132;148m[38;2;0;0;0mstretch[39m[49m       [48;2;126;132;148m[38;2;0;0;0mstart[39m[49m[48;2;126;132;148m[38;2;0;0;0m [39m[49m              gs6pb                            
      10:15:00[90m+0[39m  [48;2;233;0;228m[38;2;255;255;255menvironment[39m[49m  [48;2;195;1;99m[38;2;255;255;255m [39m[49m[48;2;16;106;108m[38;2;255;255;255mhome[39m[49m[48;2;16;106;108m[38;2;255;255;255m [39m[49m               92g32                            
      10:15:00[90m+0[39m  [48;2;126;132;148m[38;2;0;0;0mstretch[39m[49m      [48;2;126;132;148m[38;2;0;0;0m [39m[49mend                 chq8f                            
      10:18:00[90m+0[39m  [48;2;148;83;231m[38;2;255;255;255mpushup[39m[49m       [48;2;136;136;136m[38;2;102;102;102m[38;2;148;83;231m●[39m[39m[49m                    getek  10                        
      10:30:00[90m+0[39m  [48;2;233;0;228m[38;2;255;255;255menvironment[39m[49m  [48;2;16;106;108m[38;2;255;255;255m [39m[49m[48;2;195;1;99m[38;2;255;255;255moutside[39m[49m[48;2;16;106;108m[38;2;255;255;255m [39m[49m       5m   2abgf                            
      11:00:00[90m+0[39m  [48;2;231;36;248m[38;2;255;255;255mcaffeine[39m[49m     [48;2;136;136;136m[38;2;102;102;102m[38;2;231;36;248m●[39m[39m[49m                    cav6f  100               "coffee"
      11:00:00[90m+0[39m  [48;2;70;248;111m[38;2;0;0;0mproject[39m[49m       [48;2;175;103;202m[38;2;0;0;0memails[39m[49m[48;2;44;177;173m[38;2;0;0;0m,tasks[39m[49m[48;2;175;103;202m[38;2;44;177;173m▞[39m[49m       yafyz                            
      11:30:00[90m+0[39m  [48;2;70;248;111m[38;2;0;0;0mproject[39m[49m      [48;2;175;103;202m[38;2;44;177;173m▞[39m[49m[48;2;212;135;136m[38;2;0;0;0mmeeting[39m[49m[48;2;175;103;202m[38;2;44;177;173m [39m[49m       10m  989x0                            
      11:45:00[90m+0[39m  [48;2;70;248;111m[38;2;0;0;0mproject[39m[49m      [48;2;175;103;202m[38;2;44;177;173m▞[39m[49mend                 q9nko                            
      ",
      ]
    `);
  });
});
