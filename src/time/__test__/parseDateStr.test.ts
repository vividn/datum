import { parseDateStr, ParseDateStrType } from "../parseDateStr";
import { DateTime, Settings } from "luxon";
import { setNow } from "../../test-utils";

function expectDate(
  props: ParseDateStrType,
  timeObject: { year: number; month: number; day: number }
) {
  expect(parseDateStr(props).toObject()).toMatchObject(timeObject);
}

beforeEach(() => {
  setNow("2021-10-26T12:00:30Z");
});

it("handles absolute dates", () => {
  expectDate({ dateStr: "2018-06-30" }, { year: 2018, month: 6, day: 30 });
  expectDate({ dateStr: "06/30" }, { year: 2021, month: 6, day: 30 });
  expectDate({ dateStr: "may1" }, { year: 2021, month: 5, day: 1 });
  expectDate({ dateStr: "june 18" }, { year: 2021, month: 6, day: 18 });
});

it("handles absolute dates with reference time", () => {
  const referenceTime = DateTime.fromObject({
    year: 2017,
    month: 11,
    day: 29,
    hour: 6,
  });
  expectDate(
    { dateStr: "2018-06-30", referenceTime },
    { year: 2018, month: 6, day: 30 }
  );
  expectDate(
    { dateStr: "06/30", referenceTime },
    { year: 2017, month: 6, day: 30 }
  );
  expectDate(
    { dateStr: "sep1", referenceTime },
    { year: 2017, month: 9, day: 1 }
  );
  expectDate(
    { dateStr: "june 18", referenceTime },
    { year: 2017, month: 6, day: 18 }
  );
});

it("handles relative dates from now", () => {
  expectDate({ dateStr: "-1" }, { year: 2021, month: 10, day: 25 });
  expectDate({ dateStr: "+1" }, { year: 2021, month: 10, day: 27 });
  expectDate({ dateStr: "-2" }, { year: 2021, month: 10, day: 24 });
  expectDate({ dateStr: "yesterday" }, { year: 2021, month: 10, day: 25 });
  expectDate({ dateStr: "2 days ago" }, { year: 2021, month: 10, day: 24 });
  expectDate({ dateStr: "a week from now" }, { year: 2021, month: 11, day: 2 });
});

it("handles relative dates from referenceTime", () => {
  const referenceTime = DateTime.fromObject({
    year: 2017,
    month: 11,
    day: 29,
    hour: 6,
  });
  expectDate(
    { dateStr: "-1", referenceTime },
    { year: 2017, month: 11, day: 28 }
  );
  expectDate(
    { dateStr: "+3", referenceTime },
    { year: 2017, month: 12, day: 2 }
  );
  expectDate(
    { dateStr: "-2", referenceTime },
    { year: 2017, month: 11, day: 27 }
  );
  expectDate(
    { dateStr: "yesterday", referenceTime },
    { year: 2017, month: 11, day: 28 }
  );
  expectDate(
    { dateStr: "2 days ago", referenceTime },
    { year: 2017, month: 11, day: 27 }
  );
  expectDate(
    { dateStr: "a week from now", referenceTime },
    { year: 2017, month: 12, day: 6 }
  );
});

it("handles relative dates correctly even close to midnight in non-utc timezones", () => {
  Settings.defaultZone = "America/Sao_Paulo";

  // Slightly before midnight
  setNow("2021-10-26T23:00:00");
  expectDate({ dateStr: "tomorrow" }, { year: 2021, month: 10, day: 27 });

  //Slightly after
  setNow("2021-10-27T01:00:00");
  expectDate({ dateStr: "tomorrow" }, { year: 2021, month: 10, day: 28 });

  Settings.defaultZone = "system";
});
