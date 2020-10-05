import { Settings, DateTime, Zone } from "luxon";
Settings.defaultLocale = "utc";
const mockNow = DateTime.utc(2020, 5, 10, 15, 25, 30).toMillis();
Settings.now = () => mockNow;

const {processTimeArgs, combineDateTime, relTimeStr } = require('../src/timings');

describe("processTimeArgs", () => {
  it("returns current time when no arguments are given", () => {
    const result = processTimeArgs();
    expect(result).toBe("2020-05-10T15:25:30.000Z");
  });
});