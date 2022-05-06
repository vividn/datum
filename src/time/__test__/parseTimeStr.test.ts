import { DateTime, Duration, Settings } from "luxon";
import { parseTimeStr } from "../parseTimeStr";

const todayObj = { year: 2021, month: 10, day: 12 };
const timeOfDayObj = { hour: 18, minute: 32, second: 30 };
const anotherDate = { year: 2021, month: 5, day: 6 };
const anotherTime = { hour: 10, minute: 17, second: 0 };
let mockNow: DateTime;

beforeEach(() => {
  mockNow = DateTime.fromObject({ ...todayObj, ...timeOfDayObj });
  const mockNowMillis = mockNow.toMillis();
  Settings.now = () => mockNowMillis;
});

describe("absolute time strings", () => {
  test.each([
    ["3:15", { ...todayObj, hour: 3, minute: 15 }],
    ["15:15", { ...todayObj, hour: 15, minute: 15 }],
    ["3:15am", { ...todayObj, hour: 3, minute: 15 }],
    ["3:15pm", { ...todayObj, hour: 15, minute: 15 }],
    ["315", { ...todayObj, hour: 3, minute: 15 }],
    ["0315", { ...todayObj, hour: 3, minute: 15 }],
    ["2315", { ...todayObj, hour: 23, minute: 15 }],
    ["10", { ...todayObj, hour: 10 }],
    ["0", { ...todayObj, hour: 0 }],
    ["24", { ...todayObj, day: 13 }],
    [
      "16:25:20.555",
      {
        ...todayObj,
        hour: 16,
        minute: 25,
        second: 20,
        millisecond: 555,
      },
    ],
    ["16:45:04", { ...todayObj, hour: 16, minute: 45, second: 4 }],
    ["3:45:04pm", { ...todayObj, hour: 15, minute: 45, second: 4 }],
    ["2010-05-20T10:00:00Z", { year: 2010, month: 5, day: 20, hour: 10 }],
    ["yesterday at 10:17", { ...todayObj, day: 11, hour: 10, minute: 17 }],
  ])("it parses %s as DateTime from %s", (timeStr, expected) => {
    expect(parseTimeStr({ timeStr })).toEqual(DateTime.fromObject(expected));
  });

  test.each([
    ["3:15", { ...anotherDate, hour: 3, minute: 15 }],
    ["15:15", { ...anotherDate, hour: 15, minute: 15 }],
    ["3:15am", { ...anotherDate, hour: 3, minute: 15 }],
    ["3:15pm", { ...anotherDate, hour: 15, minute: 15 }],
    ["315", { ...anotherDate, hour: 3, minute: 15 }],
    ["0315", { ...anotherDate, hour: 3, minute: 15 }],
    ["2315", { ...anotherDate, hour: 23, minute: 15 }],
    ["10", { ...anotherDate, hour: 10 }],
    ["0", { ...anotherDate, hour: 0 }],
    ["24", { ...anotherDate, day: 7 }],
    [
      "16:25:20.555",
      {
        ...anotherDate,
        hour: 16,
        minute: 25,
        second: 20,
        millisecond: 555,
      },
    ],
    ["16:45:04", { ...anotherDate, hour: 16, minute: 45, second: 4 }],
    ["3:45:04pm", { ...anotherDate, hour: 15, minute: 45, second: 4 }],
    ["2010-05-20T10:00:00Z", { year: 2010, month: 5, day: 20, hour: 10 }],
    ["yesterday at 10:17", { ...anotherDate, day: 5, hour: 10, minute: 17 }],
  ])("it parses %s as DateTime from %s", (timeStr, expected) => {
    const differentReferenceTime = DateTime.fromObject({
      ...anotherDate,
      ...anotherTime,
    });
    expect(
      parseTimeStr({ timeStr, referenceTime: differentReferenceTime })
    ).toEqual(DateTime.fromObject(expected));
  });
});

describe("relative time strings", () => {
  test.each([
    ["+1", { minutes: 1 }],
    ["+10m", { minutes: 10 }],
    ["+100min", { minutes: 100 }],
    ["+87.3", { minutes: 87.3 }],
    ["-1", { minutes: -1 }],
    ["-10", { minutes: -10 }],
    ["-30 minutes", { minutes: -30 }],
    ["+3h", { hours: 3 }],
    ["-1hr", { hours: -1 }],
    ["-3hrs", { hours: -3 }],
    ["+8.5hours", { hours: 8.5 }],
    ["+8 hours", { hours: 8 }],
    ["+100s", { seconds: 100 }],
    ["-3sec", { seconds: -3 }],
  ])(
    "it parses %s as a duration of %s away from now",
    (timeStr, durationObject) => {
      const expectedDateTime = mockNow.plus(
        Duration.fromObject(durationObject)
      );
      expect(parseTimeStr({ timeStr })).toEqual(expectedDateTime);
    }
  );

  test.each([
    ["+1", { minutes: 1 }],
    ["+10m", { minutes: 10 }],
    ["+100min", { minutes: 100 }],
    ["+87.3", { minutes: 87.3 }],
    ["-1", { minutes: -1 }],
    ["-10", { minutes: -10 }],
    ["-30 minutes", { minutes: -30 }],
    ["+3h", { hours: 3 }],
    ["-1hr", { hours: -1 }],
    ["-3hrs", { hours: -3 }],
    ["+8.5hours", { hours: 8.5 }],
    ["+8 hours", { hours: 8 }],
    ["+100s", { seconds: 100 }],
    ["-3sec", { seconds: -3 }],
  ])(
    "it parses %s as a duration of %s away from a relative time",
    (timeStr, durationObject) => {
      const differentReferenceTime = DateTime.fromObject({
        ...anotherDate,
        ...anotherTime,
      });
      const expectedDateTime = differentReferenceTime.plus(
        Duration.fromObject(durationObject)
      );
      expect(
        parseTimeStr({ timeStr, referenceTime: differentReferenceTime })
      ).toEqual(expectedDateTime);
    }
  );
});
