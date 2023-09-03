import { setNow, testDbLifecycle } from "../../test-utils";
import { DateTime } from "luxon";
import * as switchCmdModle from "../switchCmd";
import { setupCmd } from "../setupCmd";
import { startCmd } from "../startCmd";

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
      occurTime: DateTime.now().toUTC().toISO(),
    });
  });

  it("calls switchCmd with state: true", async () => {
    const switchCmdSpy = jest.spyOn(switchCmdModle, "switchCmd");
    await startCmd({ field: "mop" });
    expect(switchCmdSpy).toBeCalledWith({ field: "mop", state: true });
  });
});
