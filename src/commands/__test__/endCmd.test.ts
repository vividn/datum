import { setNow, testDbLifecycle } from "../../test-utils";
import { endCmd } from "../endCmd";
import { DateTime } from "luxon";
import * as switchCmdModle from "../switchCmd";
import { setupCmd } from "../setupCmd";

describe("endCmd", () => {
  const dbName = "end_cmd_test";
  const db = testDbLifecycle(dbName);
  beforeEach(async () => {
    await setupCmd({});
    setNow("2023-09-02,11:45");
  });

  it("creates an occur document with state: false", async () => {
    const doc = await endCmd({ field: "dance" });
    expect(doc.data).toMatchObject({
      field: "dance",
      state: false,
      occurTime: DateTime.now().toUTC().toISO(),
    });
  });

  it("calls switchCmd with state: false", async () => {
    const switchCmdSpy = jest.spyOn(switchCmdModle, "switchCmd");
    await endCmd({ field: "mop" });
    expect(switchCmdSpy).toBeCalledWith({ field: "mop", state: false });
  });
});
