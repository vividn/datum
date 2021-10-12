import { Settings, DateTime, Duration } from "luxon";
import { afterEach, beforeEach, describe, expect, it } from "@jest/globals";
import timezone_mock from "timezone-mock";
import {
  BadDateArgError,
  BadTimeArgError,
  BadTimezoneError,
} from "../../src/errors";
import {
  processTimeArgs,
  ProcessTimeArgsType,
  TimingData,
} from "../../src/time/processTimeArgs";

const expectTiming = (
  props: ProcessTimeArgsType,
  expectedTimeStrOrTimings: string | Partial<TimingData>
) => {
  const expectedOutput =
    typeof expectedTimeStrOrTimings === "string"
      ? { timeStr: expectedTimeStrOrTimings }
      : expectedTimeStrOrTimings;

  expect(processTimeArgs(props)).toMatchObject(expectedOutput);
};

describe("processTimeArgs", () => {
  beforeEach(() => {
    timezone_mock.register("UTC");
    const mockNow = DateTime.utc(2020, 5, 10, 15, 25, 30).toMillis();
    Settings.now = () => mockNow;
  });

  afterEach(() => {
    timezone_mock.unregister();
    Settings.resetCaches();
  });

  it("returns timeStr as current time when no arguments are given", () => {
    expect(processTimeArgs({}).timeStr).toBe("2020-05-10T15:25:30.000Z");
  });

  it("handles absolute time strings", () => {
    expectTiming({ time: "3:15" }, "2020-05-10T03:15:00.000Z");
    expectTiming({ time: "15:15" }, "2020-05-10T15:15:00.000Z");
    expectTiming({ time: "3:15am" }, "2020-05-10T03:15:00.000Z");
    expectTiming({ time: "3:15pm" }, "2020-05-10T15:15:00.000Z");
    expectTiming({ time: "315" }, "2020-05-10T03:15:00.000Z");
    expectTiming({ time: "0315" }, "2020-05-10T03:15:00.000Z");
    expectTiming({ time: "2315" }, "2020-05-10T23:15:00.000Z");
    expectTiming({ time: "10" }, "2020-05-10T10:00:00.000Z");
    expectTiming({ time: "0" }, "2020-05-10T00:00:00.000Z");
    expectTiming({ time: "24" }, "2020-05-11T00:00:00.000Z");
    expectTiming({ time: "16:25:20.555" }, "2020-05-10T16:25:20.555Z");
    expectTiming({ time: "16:45:04" }, "2020-05-10T16:45:04.000Z");
    expectTiming({ time: "3:45:04pm" }, "2020-05-10T15:45:04.000Z");
    expectTiming({ time: "2010-05-20T10:00:00Z" }, "2010-05-20T10:00:00.000Z");
    expectTiming({ time: "yesterday at 10:17" }, "2020-05-09T10:17:00.000Z");
  });

  it("handles relative time strings", () => {
    const dtMockNow = DateTime.utc();
    expectTiming(
      { time: "+1" },
      dtMockNow.plus(Duration.fromObject({ minutes: 1 })).toString()
    );
    expectTiming(
      { time: "+10m" },
      dtMockNow.plus(Duration.fromObject({ minutes: 10 })).toString()
    );
    expectTiming(
      { time: "+100min" },
      dtMockNow.plus(Duration.fromObject({ minutes: 100 })).toString()
    );
    expectTiming(
      { time: "+87.3" },
      dtMockNow
        .plus(Duration.fromObject({ minutes: 87, seconds: 18 }))
        .toString()
    );
    expectTiming(
      { time: "-1" },
      dtMockNow.minus(Duration.fromObject({ minutes: 1 })).toString()
    );
    expectTiming(
      { time: "-10" },
      dtMockNow.minus(Duration.fromObject({ minutes: 10 })).toString()
    );
    expectTiming(
      { time: "-30 minutes" },
      dtMockNow.minus(Duration.fromObject({ minutes: 30 })).toString()
    );
    expectTiming(
      { time: "+3h" },
      dtMockNow.plus(Duration.fromObject({ hours: 3 })).toString()
    );
    expectTiming(
      { time: "-1hr" },
      dtMockNow.minus(Duration.fromObject({ hours: 1 })).toString()
    );
    expectTiming(
      { time: "-3hrs" },
      dtMockNow.minus(Duration.fromObject({ hours: 3 })).toString()
    );
    expectTiming(
      { time: "+8.5hours" },
      dtMockNow.plus(Duration.fromObject({ hours: 8.5 })).toString()
    );
    expectTiming(
      { time: "+8 hours" },
      dtMockNow.plus(Duration.fromObject({ hours: 8 })).toString()
    );
    expectTiming(
      { time: "+100s" },
      dtMockNow.plus(Duration.fromObject({ seconds: 100 })).toString()
    );
    expectTiming(
      { time: "-3sec" },
      dtMockNow.minus(Duration.fromObject({ seconds: 3 })).toString()
    );
  });

  it("handles quick args", () => {
    const dtMockNow = DateTime.utc();
    expectTiming(
      { quick: 1 },
      dtMockNow.minus(Duration.fromObject({ minutes: 5 })).toString()
    );
    expectTiming(
      { quick: 2 },
      dtMockNow.minus(Duration.fromObject({ minutes: 10 })).toString()
    );
    expectTiming(
      { quick: 6 },
      dtMockNow.minus(Duration.fromObject({ minutes: 30 })).toString()
    );
    expectTiming({ quick: 3, time: "10:15" }, "2020-05-10T10:00:00.000Z");
  });

  it("handles absolute dates", () => {
    expectTiming({ date: "2018-06-30" }, "2018-06-30");
    expectTiming({ date: "06/30" }, "2020-06-30");
    expectTiming({ date: "may1" }, "2020-05-01");
    expectTiming({ date: "june 18" }, "2020-06-18");
  });

  it("handles relative dates", () => {
    expectTiming({ date: "-1" }, "2020-05-09");
    expectTiming({ date: "+1" }, "2020-05-11");
    expectTiming({ date: "-2" }, "2020-05-08");
    expectTiming({ date: "yesterday" }, "2020-05-09");
    expectTiming({ date: "2 days ago" }, "2020-05-08");
    expectTiming({ date: "a week from now" }, "2020-05-17");
  });

  it("handles yesterday arg", () => {
    expectTiming({ yesterday: 1 }, "2020-05-09");
    expectTiming({ yesterday: 2 }, "2020-05-08");
    expectTiming({ yesterday: 5 }, "2020-05-05");
    expectTiming({ date: "2010-09-01", yesterday: 1 }, "2010-08-31");
  });

  it("can handle date and time together", () => {
    expectTiming(
      { date: "2018-06-30", time: "15:18" },
      "2018-06-30T15:18:00.000Z"
    );
    expectTiming({ date: "may1", time: "10" }, "2020-05-01T10:00:00.000Z");
    expectTiming({ date: "june 18", time: "4pm" }, "2020-06-18T16:00:00.000Z");
  });

  it("only returns date when fullDay is given", () => {
    expectTiming(
      { date: "2020-05-20", time: "15:18", fullDay: true },
      "2020-05-20"
    );
    expectTiming({ fullDay: true }, "2020-05-10");
  });

  it("gives local date, not utc date", () => {
    timezone_mock.register("Brazil/East");
    const mockNow = DateTime.utc(2020, 5, 10, 2, 0, 0).toMillis(); // 23:00 May 9, Brazil time
    Settings.now = () => mockNow;

    const result = processTimeArgs({ fullDay: true });
    expect(result.timeStr).toBe("2020-05-09");
  });

  it("adjust datetime appropriately for timezone", () => {
    // Before DST, UTC-6
    expectTiming(
      { timezone: "America/Chicago", date: "2018-03-10", time: "10:00" },
      { timeStr: "2018-03-10T16:00:00.000Z", utcOffset: -6 }
    );
    // After DST, UTC-5
    expectTiming(
      { timezone: "America/Chicago", date: "2018-03-12", time: "10:00" },
      { timeStr: "2018-03-12T15:00:00.000Z", utcOffset: -5 }
    );
    expectTiming(
      { timezone: "+4", time: "10:00" },
      { timeStr: "2020-05-10T06:00:00.000Z", utcOffset: 4 }
    );
    expectTiming(
      { timezone: "-4", time: "10:00" },
      { timeStr: "2020-05-10T14:00:00.000Z", utcOffset: -4 }
    );
    expectTiming(
      { timezone: "0", time: "10:00" },
      { timeStr: "2020-05-10T10:00:00.000Z", utcOffset: 0 }
    );
    expectTiming(
      { timezone: "2", time: "10:00" },
      { timeStr: "2020-05-10T08:00:00.000Z", utcOffset: 2 }
    );
  });

  it("throws on unparsable times and dates", () => {
    expect(() => processTimeArgs({ time: "absolute rubbish" })).toThrowError(
      BadTimeArgError
    );
    expect(() => processTimeArgs({ date: "before" })).toThrowError(
      BadDateArgError
    );
    expect(() =>
      processTimeArgs({ time: "3am", date: "the end of the universe" })
    ).toThrowError(BadDateArgError);
    expect(() =>
      processTimeArgs({ time: "half past nothing", yesterday: 1 })
    ).toThrowError(BadTimeArgError);
    expect(() =>
      processTimeArgs({ timezone: "rubbish/timezone" })
    ).toThrowError(BadTimezoneError);
  });

  it("does not persist timezone across runs", () => {
    const { utcOffset: offset1 } = processTimeArgs({ timezone: "+3" });
    expect(offset1).toEqual(3);
    const { utcOffset: offset2 } = processTimeArgs({});
    expect(offset2).toEqual(0);
  });

  it("returns an undefined timeStr if noTimestamp is requested", () => {
    const { timeStr } = processTimeArgs({ noTimestamp: true });
    expect(timeStr).toBeUndefined();
  });

  it("returns the locales timezone if none is specified", () => {
    const { utcOffset: offset1 } = processTimeArgs({});
    expect(offset1).toBe(0);

    timezone_mock.register("Brazil/East");
    const { utcOffset: offset2 } = processTimeArgs({});
    expect(offset2).toBe(-3);

    const { utcOffset: offset3 } = processTimeArgs({ noTimestamp: true });
    expect(offset3).toBe(-3);
  });
});
