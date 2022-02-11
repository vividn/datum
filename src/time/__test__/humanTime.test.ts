import { DateTime, Settings } from "luxon";
import { humanTime } from "../humanTime";

const mockNow = DateTime.utc(2022, 2, 11, 9, 20, 0);

beforeEach(async () => {
  Settings.now = () => mockNow.toMillis();
  Settings.defaultZone = "system";
});

it("displays HH:mm:ss if the DateTime is today", () => {
  const tenThirtyOrSo = DateTime.local(2022, 2, 11, 10, 30, 19);
  expect(humanTime(tenThirtyOrSo)).toEqual("10:30:19");
});

it("displays -1d, HH:mm:ss if yesterday", () => {
  const yesterdayAfternoon = DateTime.local(2022, 2, 10, 16, 22);
  expect(humanTime(yesterdayAfternoon)).toEqual("-1d, 16:22:00");
});

it("displays +1d, HH:mm:ss if tomorrow", () => {
  const tomorrowMorning = DateTime.local(2022, 2, 12, 7, 0, 0);
  expect(humanTime(tomorrowMorning)).toEqual("+1d, 07:00:00");
});

it("displays MMM d, HH:mm:ss if same year", () => {
  const summerSolstice = DateTime.local(2022, 6, 21, 11, 13);
  expect(humanTime(summerSolstice)).toEqual("Jun 21, 11:13:00");
});

it("displays yyyy-MM-dd, HH:mm:ss if different year", () => {
  const lastYearWinterSolstice = DateTime.local(2021, 12, 21, 15, 59);
  expect(humanTime(lastYearWinterSolstice)).toEqual("2021-12-21, 15:59:00");
});

it("adds UTC+N to end if utc offset does not match locale", () => {
  Settings.defaultZone = "UTC+7";
  const zone5 = DateTime.local(2022, 2, 11, 10, 40, { zone: "UTC+5" });
  expect(humanTime(zone5)).toEqual("10:40:00 UTC+5");

  const chicagoTime = DateTime.local(2022, 5, 20, 19, 34, 12, {
    zone: "America/Chicago",
  });
  expect(humanTime(chicagoTime)).toEqual("May 20, 19:34:12 UTC-5");

  const utcTime = DateTime.utc(2020, 10, 10, 5, 5, 25);
  expect(humanTime(utcTime)).toEqual("2020-10-10, 05:05:25 UTC+0");
});

it("does not add UTC+N if the offset is the same as locale, even if the timezone is technically different", () => {
  Settings.defaultZone = "Europe/Berlin";
  const copenhagenTime = DateTime.local(2022, 2, 11, 17, 22, {
    zone: "Europe/Copenhagen",
  });
  expect(humanTime(copenhagenTime)).toEqual("17:22:00");
});

it("still thinks it's today even if utc date is different than local date", () => {
  const mockUtcAnotherDay = DateTime.utc(2022, 2, 13, 23, 30);
  Settings.now = () => mockUtcAnotherDay.toMillis();
  Settings.defaultZone = "Europe/Berlin";
  const alreadyValentinesDay = DateTime.local();
  expect(humanTime(alreadyValentinesDay)).toEqual("00:30:00");
});
