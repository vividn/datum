import { testDbLifecycle } from "../../__test__/test-utils.js";
import { addCmd } from "../../commands/addCmd.js";
import { occurCmd } from "../../commands/occurCmd.js";
import { setupCmd } from "../../commands/setupCmd.js";
import { startCmd } from "../../commands/startCmd.js";
import { switchCmd } from "../../commands/switchCmd.js";
import { occurredFields } from "../occurredFields.js";

describe("occurredFields", () => {
  const db = testDbLifecycle("occurred_fields_test");
  beforeEach(async () => {
    await setupCmd({});
  });

  it("should return any fields that have occurred or had some state change, but not any that have just been added", async () => {
    await occurCmd("occurredField");
    await startCmd("startedField");
    await switchCmd("switchedField state");
    await addCmd("addedField");
    await addCmd("addedAndOccurredField");
    await occurCmd("addedAndOccurredField");

    const fieldList = await occurredFields(db);
    expect(fieldList).toEqual([
      "addedAndOccurredField",
      "occurredField",
      "startedField",
      "switchedField",
    ]);
  });
});
