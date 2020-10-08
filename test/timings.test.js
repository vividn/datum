import { Settings, DateTime, Zone, Duration } from "luxon";
// Settings.defaultZoneName = "utc";
const timezone_mock = require("timezone-mock");
timezone_mock.register("UTC");
const mockNow = DateTime.utc(2020, 5, 10, 15, 25, 30).toMillis();
Settings.now = () => mockNow;
const dtMockNow = DateTime.utc();

const {
  processTimeArgs,
  combineDateTime,
  relTimeStr,
} = require("../src/timings");

describe("processTimeArgs", () => {
  it("returns current time when no arguments are given", () => {
    expect(processTimeArgs({})).toBe("2020-05-10T15:25:30.000Z");
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
    testCases.forEach((testCase) => {
      expect(processTimeArgs(testCase[0]), `${JSON.stringify(testCase[0])}`).toBe(testCase[1]);
    });
  });

  it("handles relative time strings", () => {
    // prettier: ignore
    const testCases = [
      [{ time: "+1" }, dtMockNow.plus(Duration.fromObject({ minutes: 1 })).toString()],
      [{ time: "+10m" }, dtMockNow.plus(Duration.fromObject({ minutes: 10 })).toString()],
      [{ time: "+100min" }, dtMockNow.plus(Duration.fromObject({ minutes: 100 })).toString()],
      [{ time: "+87.3" },dtMockNow.plus(Duration.fromObject({ minutes: 87, seconds: 18 })).toString()],
      [{ time: "-1" }, dtMockNow.minus(Duration.fromObject({ minutes: 1 })).toString()],
      [{ time: "-10" }, dtMockNow.minus(Duration.fromObject({ minutes: 10 })).toString()],
      [{ time: "-30 minutes" }, dtMockNow.minus(Duration.fromObject({ minutes: 30 })).toString()],
      [{ time: "+3h" }, dtMockNow.plus(Duration.fromObject({ hours: 3 })).toString()],
      [{ time: "-1hr" }, dtMockNow.minus(Duration.fromObject({ hours: 1 })).toString()],
      [{ time: "-3hrs" }, dtMockNow.minus(Duration.fromObject({ hours: 3 })).toString()],
      [{ time: "+8.5hours" }, dtMockNow.plus(Duration.fromObject({ hours: 8.5 })).toString()],
      [{ time: "+8 hours" }, dtMockNow.plus(Duration.fromObject({ hours: 8 })).toString()],
      [{ time: "+100s" }, dtMockNow.plus(Duration.fromObject({ seconds: 100 })).toString()],
      [{ time: "-3sec" }, dtMockNow.minus(Duration.fromObject({ seconds: 3 })).toString()],
    ];

    testCases.forEach((testCase) => {
      expect(processTimeArgs(testCase[0]), `${JSON.stringify(testCase[0])}`).toBe(testCase[1]);
    });
  });

  it("handles quick args", () => {
    const testCases = [
      [{ quick: 1 }, dtMockNow.minus(Duration.fromObject({ minutes: 5 })).toString()],
      [{ quick: 2 }, dtMockNow.minus(Duration.fromObject({ minutes: 10 })).toString()],
      [{ quick: 6 }, dtMockNow.minus(Duration.fromObject({ minutes: 30 })).toString()],
      [{ quick: 3, time: "10:15" }, "2020-05-10T10:00:00.000Z"]
    ];
    testCases.forEach((testCase) => {
      expect(processTimeArgs(testCase[0]), `${JSON.stringify(testCase[0])}`).toBe(testCase[1]);
    });
  });

  it("handles absolute dates", () => {
    const testCases = [
      [{ date: "2018-06-30" }, "2018-06-30"],
      [{ date: "06/30" }, "2020-06-30"],
      [{ date: "may1" }, "2020-05-01"],
      [{ date: "june 18" }, "2020-06-18"],
    ];
    testCases.forEach((testCase) => {
      expect(processTimeArgs(testCase[0]), `${JSON.stringify(testCase[0])}`).toBe(testCase[1]);
    });
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
      testCases.forEach((testCase) => {
        expect(processTimeArgs(testCase[0]), `${JSON.stringify(testCase[0])}`).toBe(testCase[1]);
      });
    });

    it("handles yesterday arg", () => {
      const testCases = [
        [{ yesterday: 1 }, "2020-05-09"],
        [{ yesterday: 2 }, "2020-05-08"],
        [{ yesterday: 5 }, "2020-05-05"],
        [{ date: "2010-09-01", yesterday: 1 }, "2010-08-31"],
      ];
      testCases.forEach((testCase) => {
        expect(processTimeArgs(testCase[0]), `${JSON.stringify(testCase[0])}`).toBe(testCase[1]);
      });
    });

it("can handle date and time together", () => {
    const testCases = [
      [{ date: "2018-06-30", time: "15:18" }, "2018-06-30T15:18:00.000Z"],
      [{ date: "may1", time: "10" }, "2020-05-01T10:00:00.000Z"],
      [{ date: "june 18", time: "4pm" }, "2020-06-18T16:00:00.000Z"]
    ];
    testCases.forEach((testCase) => {
      expect(processTimeArgs(testCase[0]), `${JSON.stringify(testCase[0])}`).toBe(testCase[1]);
    });
  })
});
