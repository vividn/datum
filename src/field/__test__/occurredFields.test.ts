import { testDbLifecycle } from "../../__test__/test-utils";
import { addCmd } from "../../commands/addCmd";
import { occurCmd } from "../../commands/occurCmd";
import { setupCmd } from "../../commands/setupCmd";
import { startCmd } from "../../commands/startCmd";
import { switchCmd } from "../../commands/switchCmd";
import { occurredFields } from "../occurredFields";

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
