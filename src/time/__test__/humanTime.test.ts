import { DateTime, Settings } from "luxon";
import { humanTime } from "../humanTime";
import { setNow } from "../../__test__/test-utils";
import { toDatumTime } from "../timeUtils";
import chalk from "chalk";

describe("humanTime", () => {
  beforeEach(async () => {
    setNow("2022-02-11T09:20:00Z");
    Settings.defaultZone = "system";
    process.env["FORCE_COLOR"] = "0";
    chalk.level = 0;
  });

  it("displays HH:mm:ss if the DateTime is today", () => {
    const tenThirtyOrSo = toDatumTime("today, 10:30:19");
    expect(humanTime(tenThirtyOrSo)).toEqual("10:30:19+0");
  });

  it("displays -1d HH:mm:ss if yesterday", () => {
    const yesterdayAfternoon = toDatumTime("yesterday, 16:22:00");
    expect(humanTime(yesterdayAfternoon)).toEqual("-1d 16:22:00+0");
  });

  it("displays +1d HH:mm:ss if tomorrow", () => {
    const tomorrowMorning = toDatumTime("tomorrow, 07:00:00");
    expect(humanTime(tomorrowMorning)).toEqual("+1d 07:00:00+0");
  });

  it("displays up to 3 days in the future or past with this format", () => {
    expect(humanTime(toDatumTime("2022-02-14, 10:00:00"))).toEqual(
      "+3d 10:00:00+0",
    );
    expect(humanTime(toDatumTime("2022-02-08, 10:00:00"))).toEqual(
      "-3d 10:00:00+0",
    );
  });

  it("if a time is 4 or more days in the future or past, than it displays the iso date", () => {
    expect(humanTime(toDatumTime("2022-02-15, 10:00:00"))).toEqual(
      "2022-02-15 10:00:00+0",
    );
    expect(humanTime(toDatumTime("2022-02-07, 10:00:00"))).toEqual(
      "2022-02-07 10:00:00+0",
    );
  });

  it("adds an hour offset indicator after the time", () => {
    Settings.defaultZone = "UTC+7";
    const zone5 = toDatumTime(
      DateTime.local(2022, 2, 11, 10, 40, { zone: "UTC+5" }),
    );
    expect(humanTime(zone5)).toEqual("10:40:00+5");

    const chicagoTime = toDatumTime(
      DateTime.local(2022, 5, 20, 19, 34, 12, {
        zone: "America/Chicago",
      }),
    );
    expect(humanTime(chicagoTime)).toEqual("2022-05-20 19:34:12-5");

    const utcTime = toDatumTime(DateTime.utc(2020, 10, 10, 5, 5, 25));
    expect(humanTime(utcTime)).toEqual("2020-10-10 05:05:25+0");
  });

  it("still thinks it's today even if utc date is different than local date", () => {
    setNow("2022-02-13T23:30:00Z");
    Settings.defaultZone = "Europe/Berlin";
    const alreadyValentinesDay = toDatumTime(DateTime.local());
    expect(humanTime(alreadyValentinesDay)).toEqual("00:30:00+1");
  });
});
