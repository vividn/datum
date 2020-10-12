import { Settings, DateTime, Zone, Duration } from "luxon";
const timezone_mock = require("timezone-mock");
const { processTimeArgs } = require("../src/timings");

const expectTimingFromCases = (testCases) => {
  testCases.forEach((testCase) => {
    const params = testCase[0];
    const expectedOutput =
      typeof testCase[1] === "string"
        ? { occurTime: testCase[1] }
        : testCase[1];

    expect(processTimeArgs(params), `${JSON.stringify(params)}`).toMatchObject(
      expectedOutput
    );
  });
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

  it("returns occurTime as current time when no arguments are given", () => {
    expect(processTimeArgs({}).occurTime).toBe("2020-05-10T15:25:30.000Z");
  });

  it("handles absolute time strings", () => {
    const testCases = [
      [{ time: "3:15" }, "2020-05-10T03:15:00.000Z"],
      [{ time: "15:15" }, "2020-05-10T15:15:00.000Z"],
      [{ time: "3:15am" }, "2020-05-10T03:15:00.000Z"],
      [{ time: "3:15pm" }, "2020-05-10T15:15:00.000Z"],
      [{ time: "315" }, "2020-05-10T03:15:00.000Z"],
      [{ time: "0315" }, "2020-05-10T03:15:00.000Z"],
      [{ time: "2315" }, "2020-05-10T23:15:00.000Z"],
      [{ time: "10" }, "2020-05-10T10:00:00.000Z"],
      [{ time: "0" }, "2020-05-10T00:00:00.000Z"],
      [{ time: "24" }, "2020-05-11T00:00:00.000Z"],
      [{ time: "16:25:20.555" }, "2020-05-10T16:25:20.555Z"],
      [{ time: "16:45:04" }, "2020-05-10T16:45:04.000Z"],
      [{ time: "3:45:04pm" }, "2020-05-10T15:45:04.000Z"],
      [{ time: "2010-05-20T10:00:00Z" }, "2010-05-20T10:00:00.000Z"],
      [{ time: "yesterday at 10:17" }, "2020-05-09T10:17:00.000Z"],
    ];
    expectTimingFromCases(testCases);
  });

  it("handles relative time strings", () => {
    const dtMockNow = DateTime.utc();
    const testCases = [
      [
        { time: "+1" },
        dtMockNow.plus(Duration.fromObject({ minutes: 1 })).toString(),
      ],
      [
        { time: "+10m" },
        dtMockNow.plus(Duration.fromObject({ minutes: 10 })).toString(),
      ],
      [
        { time: "+100min" },
        dtMockNow.plus(Duration.fromObject({ minutes: 100 })).toString(),
      ],
      [
        { time: "+87.3" },
        dtMockNow
          .plus(Duration.fromObject({ minutes: 87, seconds: 18 }))
          .toString(),
      ],
      [
        { time: "-1" },
        dtMockNow.minus(Duration.fromObject({ minutes: 1 })).toString(),
      ],
      [
        { time: "-10" },
        dtMockNow.minus(Duration.fromObject({ minutes: 10 })).toString(),
      ],
      [
        { time: "-30 minutes" },
        dtMockNow.minus(Duration.fromObject({ minutes: 30 })).toString(),
      ],
      [
        { time: "+3h" },
        dtMockNow.plus(Duration.fromObject({ hours: 3 })).toString(),
      ],
      [
        { time: "-1hr" },
        dtMockNow.minus(Duration.fromObject({ hours: 1 })).toString(),
      ],
      [
        { time: "-3hrs" },
        dtMockNow.minus(Duration.fromObject({ hours: 3 })).toString(),
      ],
      [
        { time: "+8.5hours" },
        dtMockNow.plus(Duration.fromObject({ hours: 8.5 })).toString(),
      ],
      [
        { time: "+8 hours" },
        dtMockNow.plus(Duration.fromObject({ hours: 8 })).toString(),
      ],
      [
        { time: "+100s" },
        dtMockNow.plus(Duration.fromObject({ seconds: 100 })).toString(),
      ],
      [
        { time: "-3sec" },
        dtMockNow.minus(Duration.fromObject({ seconds: 3 })).toString(),
      ],
    ];
    expectTimingFromCases(testCases);
  });

  it("handles quick args", () => {
    const dtMockNow = DateTime.utc();
    const testCases = [
      [
        { quick: 1 },
        dtMockNow.minus(Duration.fromObject({ minutes: 5 })).toString(),
      ],
      [
        { quick: 2 },
        dtMockNow.minus(Duration.fromObject({ minutes: 10 })).toString(),
      ],
      [
        { quick: 6 },
        dtMockNow.minus(Duration.fromObject({ minutes: 30 })).toString(),
      ],
      [{ quick: 3, time: "10:15" }, "2020-05-10T10:00:00.000Z"],
    ];
    expectTimingFromCases(testCases);
  });

  it("handles absolute dates", () => {
    const testCases = [
      [{ date: "2018-06-30" }, "2018-06-30"],
      [{ date: "06/30" }, "2020-06-30"],
      [{ date: "may1" }, "2020-05-01"],
      [{ date: "june 18" }, "2020-06-18"],
    ];
    expectTimingFromCases(testCases);
  });

  it("handles relative dates", () => {
    const testCases = [
      [{ date: "-1" }, "2020-05-09"],
      [{ date: "+1" }, "2020-05-11"],
      [{ date: "-2" }, "2020-05-08"],
      [{ date: "yesterday" }, "2020-05-09"],
      [{ date: "2 days ago" }, "2020-05-08"],
      [{ date: "a week from now" }, "2020-05-17"],
    ];
    expectTimingFromCases(testCases);
  });

  it("handles yesterday arg", () => {
    const testCases = [
      [{ yesterday: 1 }, "2020-05-09"],
      [{ yesterday: 2 }, "2020-05-08"],
      [{ yesterday: 5 }, "2020-05-05"],
      [{ date: "2010-09-01", yesterday: 1 }, "2010-08-31"],
    ];
    expectTimingFromCases(testCases);
  });

  it("can handle date and time together", () => {
    const testCases = [
      [{ date: "2018-06-30", time: "15:18" }, "2018-06-30T15:18:00.000Z"],
      [{ date: "may1", time: "10" }, "2020-05-01T10:00:00.000Z"],
      [{ date: "june 18", time: "4pm" }, "2020-06-18T16:00:00.000Z"],
    ];
    expectTimingFromCases(testCases);
  });

  it("only returns date when fullDay is given", () => {
    const testCases = [
      [{ date: "2020-05-20", time: "15:18", fullDay: true }, "2020-05-20"],
      [{ fullDay: true }, "2020-05-10"],
    ];
    expectTimingFromCases(testCases);
  });

  it("gives local date, not utc date", () => {
    timezone_mock.register("Brazil/East");
    const mockNow = DateTime.utc(2020, 5, 10, 2, 0, 0).toMillis(); // 23:00 May 9, Brazil time
    Settings.now = () => mockNow;

    const result = processTimeArgs({ fullDay: true });
    expect(result.occurTime).toBe("2020-05-09");
  });

  it("adjust datetime appropriately for timezone", () => {
    const testCases = [
      // Before DST, UTC-6
      [
        { timezone: "America/Chicago", date: "2018-03-10", time: "10:00" },
        { occurTime: "2018-03-10T16:00:00.000Z", utcOffset: -6 },
      ],
      // After DST, UTC-5
      [
        { timezone: "America/Chicago", date: "2018-03-12", time: "10:00" },
        { occurTime: "2018-03-12T15:00:00.000Z", utcOffset: -5 },
      ],
      [
        { timezone: "+4", time: "10:00" },
        { occurTime: "2020-05-10T06:00:00.000Z", utcOffset: 4 },
      ],
      [
        { timezone: "-4", time: "10:00" },
        { occurTime: "2020-05-10T14:00:00.000Z", utcOffset: -4 },
      ],
    ];
    expectTimingFromCases(testCases);
  });

  it("always sets createTime and modifyTime to now", () => {
    const correctMetaTimes = {
      createTime: DateTime.utc().toString(),
      modifyTime: DateTime.utc().toString(),
    };
    const testCases = [
      [{ timezone: "+4", time: "10:00" }, correctMetaTimes],
      [{ date: "june 18", time: "4pm" }, correctMetaTimes],
      [{ time: "16:25:20.555" }, correctMetaTimes],
      [{ yesterday: 5 }, correctMetaTimes],
    ];
    expectTimingFromCases(testCases);
  });
});
