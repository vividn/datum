import { restoreNow, setNow } from "../../test-utils";

describe("occurCmd", () => {
  beforeAll(() => {
    setNow("2023-08-05T16:00:00.000Z");
  });
  afterAll(() => {
    restoreNow();
  });

  it.todo("creates a document with an occurTime");
  it.todo(
    "does not interpret the first argument after field as duration if args.moment is true"
  );
  it.todo("with no-timestamp argument it is equalivalent to an addCmd");
  it.todo("interprets a duration of 'start' or 'end' as a start or end command");
});

