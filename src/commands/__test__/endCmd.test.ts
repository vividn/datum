import { setNow, testDbLifecycle } from "../../test-utils";
import { endCmd } from "../endCmd";
import { DateTime } from "luxon";
import { setupCmd } from "../setupCmd";
import { switchCmd } from "../switchCmd";

describe("endCmd", () => {
  const dbName = "end_cmd_test";
  testDbLifecycle(dbName);
  beforeEach(async () => {
    await setupCmd({});
    setNow("2023-09-02,11:45");
  });

  it("creates an occur document with state: false", async () => {
    const doc = await endCmd({ field: "dance" });
    expect(doc.data).toMatchObject({
      field: "dance",
      state: false,
      occurTime: {
        utc: DateTime.now().toUTC().toISO(),
      },
    });
  });

  it("can end from a different state", async () => {
    await switchCmd({ field: "activity", state: "cello" });
    setNow("+10");
    const doc = await endCmd({ field: "activity" });
    expect(doc.data).toMatchObject({
      field: "activity",
      state: false,
      lastState: "cello",
      occurTime: {
        utc: DateTime.now().toUTC().toISO(),
      },
    });
  });
});
