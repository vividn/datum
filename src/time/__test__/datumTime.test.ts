describe("datumTimeToLuxon", () => {
  it("converts a DatumTime with just utc to a utc DateTime");
  it("converts a DatumTime with utc, o to a DateTime with the given offset");
  it("converts a DatumTime with utc, tz to a DateTime with the given timezone");
  it("converts a DatumTime with utc, o and tz that match to a DateTime with the given timezone");
  test("if o and tz do not match, warns about it and uses the fixed offset");
});

describe("toDatumTime", () => {
  it("converts a DateTime to a DatumTime with proper utc");
  it("records the hour offset and iana timezone as well for various DateTimes");
  it("returns a DatumTime with a date only utc value if onlyDate is passed as a parameter");
  test("the date returned matches the local date, not the utc date");
})