describe("occurCmd", () => {
  it("creates a document with an occurTime");
  it(
    "does not interpret the first argument after field as duration if args.moment is true"
  );
  it("with no-timestamp argument it is equalivalent to an addCmd");
  it("interprets a duration of 'start' or 'end' as a start or end command");
});

describe("startCmd", () => {
  it("creates an occur document with state: true");
});

describe("endCmd", () => {
  it("creates an occur document with state: false");
});

describe("switchCmd", () => {
  it("creates an occur document with a custom state");
  it("also records what it thinks the last state was");
});
