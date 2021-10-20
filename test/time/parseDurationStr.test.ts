import { parseDurationStr } from "../../src/time/parseDurationString";
import { Duration } from "luxon";
import { BadDurationArgError } from "../../src/errors";

test.each([
  ["3", {minutes: 3}],
  ["-2", {minutes: -2}],
  ["+7", {minutes: 7}],
  ["2:45", {minutes: 2, seconds: 45}],
  ["-1:12:37", {hours: -1, minutes: -12, seconds: -37}],
  ["10:11:12:13", {days: 10, hours: 11, minutes: 12, seconds: 13}],
  ["1:2:3:4:5", {years: 1, days: 2, hours: 3, minutes: 4, seconds: 5}],
  ["+0:45", {seconds: 45}],
  ["+:45", {seconds: 45}],
  ["4.5m", {minutes: 4, seconds: 30}],
  ["-30 minutes", {minutes: -30}],
  ["+3h", { hours: 3 }],
  ["-1hr", { hours: -1 }],
  ["-3hrs", { hours: -3 }],
  ["+8.5hours", { hours: 8, minutes: 30 }],
  ["+8 hours", { hours: 8 }],
  ["+100s", { minutes: 1, seconds: 40 }],
  ["-3sec", { seconds: -3 }],
  ["1m45s", {minutes: 1, seconds: 45}],
  ["1m,45s", {minutes: 1, seconds: 45}],
  ["1m 45s", {minutes: 1, seconds: 45}],
  ["3hrs 3 minutes and 2s", {hours: 3, minutes: 3, seconds: 2}],
  ["PT7M", {minutes: 7}],
  ["P3DT4H5S", {days: 3, hours: 4, seconds: 5}],
  ["-PT3M4S", {minutes: -3, seconds: -4}],
  ["PT-5H-3M", {hours: -5, minutes: -3}],
  ["307 years, 36 days, 3 seconds", {years: 307, days: 36, seconds: 3}],
  ["134 seconds", {minutes: 2, seconds: 14}],
  ["13.3", {minutes: 13, seconds: 18}],
  ["-10.25", {minutes: -10, seconds: -15}]
])("it parses %s as a Duration of length %s", (durationStr, durationObject) => {
  expect(parseDurationStr({durationStr}).toJSON()).toEqual(Duration.fromObject(durationObject).toJSON());
});

it("throws an error for a completely unparseable duration", () => {
  expect(() => parseDurationStr({ durationStr: "not a good duration" })).toThrowError(BadDurationArgError);
})