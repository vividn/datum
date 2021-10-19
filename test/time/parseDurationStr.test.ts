import { parseDurationStr } from "../../src/time/parseDurationString";
import { Duration } from "luxon";

test.each([
  ["3", {minutes: 3}],
  ["-2", {minutes: -2}],
  ["+7", {minutes: 7}],
  ["2:45", {minutes: 2, seconds: 45}],
  ["-1:12:37", {hours: -1, minutes: -12, seconds: -37}],
  ["+0:45", {seconds: 45}],
  ["+:45", {seconds: 45}],
  ["4.5m", {minutes: 4, seconds: 30}],
  ["-30 minutes", {minutes: -30}],
  ["+3h", { hours: 3 }],
  ["-1hr", { hours: -1 }],
  ["-3hrs", { hours: -3 }],
  ["+8.5hours", { hours: 8.5 }],
  ["+8 hours", { hours: 8 }],
  ["+100s", { seconds: 100 }],
  ["-3sec", { seconds: -3 }],
  ["1m45s", {minutes: 1, seconds: 45}],
  ["1m,45s", {minutes: 1, seconds: 45}],
  ["1m 45s", {minutes: 1, seconds: 45}],
  ["3hrs 3 minutes and 2s", {hours: 3, minutes: 3, seconds: 2}],
  ["PT7M", {minutes: 7}],
  ["P3DT4H5S", {days: 3, hours: 4, seconds: 5}],
  ["-PT3M4S", {minutes: -3, seconds: -4}],
  ["PT-5H-3M", {hours: -5, minutes: -3}]
])("it parses %s as a Duration of length %s", (durationStr, durationObject) => {
  expect(parseDurationStr({durationStr})).toEqual(Duration.fromObject(durationObject));
})