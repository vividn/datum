import { keyValueView } from "../keyValue";
import { testDbLifecycle } from "../../__test__/test-utils";
import { setupCmd } from "../../commands/setupCmd";
import { insertDatumView } from "../../views/insertDatumView";
import { addCmd } from "../../commands/addCmd";

describe("keyIncrement and keyDecrement", () => {
  const db = testDbLifecycle("key_epsilon_test");
  const sortedKeys = [
    null,
    false,
    true,
    -Infinity,
    -Number.MAX_VALUE,
    -9e99,
    -100,
    -1,
    -Number.MIN_VALUE,
    0,
    Number.MIN_VALUE,
    1,
    100,
    9e99,
    Number.MAX_VALUE,
    +Infinity,
    "",
    "a",
    "a`",
    "a\u0001",
    "aa",
    "a\uffff",
    "b",
  ];

  beforeEach(async () => {
    await setupCmd({});
    await insertDatumView({ db, datumView: keyValueView });
    for (const key of sortedKeys) {
      const doc = await addCmd({
        baseData: { key, value: null },
        idParts: [String(Math.random())],
      });
      console.debug(doc);
    }
  });

  it("sorts keys in the expected order", async () => {
    console.debug(await db.query(keyValueView.name, { reduce: false }));
    const keys = (
      await db.query(keyValueView.name, { reduce: false })
    ).rows.map((row) => row.key);
    expect(keys).toHaveLength(sortedKeys.length);
    expect(keys).toEqual(sortedKeys);
  });
});
