import { DateTime, Settings } from "luxon";
import { humanTime } from "../humanTime";
import { setNow } from "../../__test__/test-utils";
import { toDatumTime } from "../timeUtils";

describe("humanTime", () => {
  beforeEach(async () => {
    setNow("2022-02-11T09:20:00Z");
    Settings.defaultZone = "system";
  });

  it("displays HH:mm:ss if the DateTime is today", () => {
    const tenThirtyOrSo = toDatumTime("today,10:30:19");
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

  it("displays up to 3 days in the future or past with this format", () => {
    fail()
  });

  it("if a time is 4 or more days in the future or past, than it displays the iso date", () => {
    fail()
  });

  it("adds an hour offset indicator after the time", () => {
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

  it("still thinks it's today even if utc date is different than local date", () => {
    setNow("2022-02-13T23:30:00Z");
    Settings.defaultZone = "Europe/Berlin";
    const alreadyValentinesDay = DateTime.local();
    expect(humanTime(alreadyValentinesDay)).toEqual("00:30:00");
  });
});
