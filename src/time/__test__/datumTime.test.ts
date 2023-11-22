describe("datumTimeToLuxon", () => {
  it("converts a DatumTime with just utc to a utc DateTime");
  it("converts a DatumTime with utc, o to a DateTime with the given offset");
  it("converts a DatumTime with utc, tz to a DateTime with the given timezone");
  it("converts a DatumTime with utc, o and tz that match to a DateTime with the given timezone");
  test("if o and tz do not match, warns about it and uses the fixed offset");
})