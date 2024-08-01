import { testDbLifecycle } from "../../__test__/test-utils";
import { setupCmd } from "../../commands/setupCmd";
import { addCmd } from "../../commands/addCmd";
import { migrateOne } from "../migrateOne";
import { MigrationOps } from "../migrations";
import { DatumDocument } from "../../documentControl/DatumDocument";

describe("migrateOne", () => {
  const db = testDbLifecycle("migrate_one_test");

  let originalDoc: DatumDocument;
  let docId: string;
  beforeEach(async () => {
    await setupCmd({});
    originalDoc = (await addCmd("field key=value")) as DatumDocument;
    docId = originalDoc._id;
  });

  it("can migrate a document in the db using a row from a migration view", async () => {
    const row = {
      key: docId,
      id: docId,
      value: { op: "update" as MigrationOps, data: { key: "new value" } },
    };
    const newDoc = await migrateOne({ row, db });
    expect(newDoc.data.key).toBe("new value");

    const newDbDoc = await db.get(docId);
    expect(newDbDoc.data.key).toBe("new value");
    expect(newDbDoc.meta.humanId).toBe(originalDoc.meta.humanId);
  });

  it("can overwrite a document", async () => {
    const row = {
      key: docId,
      id: docId,
      value: {
        op: "overwrite" as MigrationOps,
        data: {
          _id: "new_id",
          data: { differentKey: "new value" },
          meta: { humanId: "newHid" },
        },
      },
    };
    const newDoc = await migrateOne({ row, db });
    expect(newDoc.data.differentKey).toBe("new value");
    expect(newDoc.data.key).toBeUndefined();

    expect(db.get(docId)).rejects.toThrow();

    const newDbDoc = await db.get("new_id");
    expect(newDbDoc.meta.humanId).toBe("newHid");
  });

  it.todo("can delete a document");

  it.todo("can apply an update to a document");

  it.todo("throws an error if the update operator is not recognized");
});
