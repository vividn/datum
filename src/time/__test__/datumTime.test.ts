import { toDatumTime } from "../datumTime";
describe("datumTimeToLuxon", () => {
  it.todo("converts a DatumTime with just utc to a utc DateTime");
  it.todo(
    "converts a DatumTime with utc, o to a DateTime with the given offset",
  );
  it.todo(
    "converts a DatumTime with utc, tz to a DateTime with the given timezone",
  );
  it.todo(
    "converts a DatumTime with utc, o and tz that match to a DateTime with the given timezone",
  );
  test.todo(
    "if o and tz do not match, warns about it and uses the fixed offset",
  );
});

describe("toDatumTime", () => {
  it.todo("converts a DateTime to a DatumTime with proper utc");
  it.todo(
    "records the hour offset and iana timezone as well for various DateTimes",
  );
  it.todo(
    "returns a DatumTime with a date only utc value if onlyDate is passed as a parameter",
  );
  test.todo("the date returned matches the local date, not the utc date");
  test("it can handle an ISO string with an offset", () => {
    expect(toDatumTime("2024-01-23T18:00:00.000-12:00")).toEqual({
      utc: "2024-01-24T06:00:00.000Z",
      o: -12,
      tz: "UTC-12",
    });
  });
});
