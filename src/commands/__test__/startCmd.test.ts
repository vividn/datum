import { setNow, testDbLifecycle } from "../../test-utils";
import { DateTime } from "luxon";
import { setupCmd } from "../setupCmd";
import { startCmd } from "../startCmd";
import { switchCmd } from "../switchCmd";

describe("startCmd", () => {
  const dbName = "start_cmd_test";
  testDbLifecycle(dbName);
  beforeEach(async () => {
    await setupCmd({});
    setNow("2023-09-02,12:30");
  });

  it("creates an occur document with state: true", async () => {
    const doc = await startCmd({ field: "dance" });
    expect(doc.data).toMatchObject({
      field: "dance",
      state: true,
      occurTime: {
        utc: DateTime.now().toUTC().toISO()
      },
    });
  });

  it("can start from a different state", async () => {
    await switchCmd({ field: "machine", state: "preparing" });
    setNow("+1");
    const doc = await startCmd({ field: "machine" });
    expect(doc.data).toMatchObject({
      field: "machine",
      state: true,
      lastState: "preparing",
      occurTime: {
        utc: DateTime.now().toUTC().toISO()
      },
    });
  });
});
