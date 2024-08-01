import { testDbLifecycle } from "../../__test__/test-utils";
import { setupCmd } from "../../commands/setupCmd";
import { addCmd } from "../../commands/addCmd";
import { migrateOne } from "../migrateOne";
import { MigrationOps } from "../migrations";
import { DatumDocument } from "../../documentControl/DatumDocument";
import { MigrationError } from "../../errors";

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
    expect(newDoc.meta.humanId).toBe(originalDoc.meta.humanId);

    const newDbDoc = await db.get(docId);
    expect(newDbDoc).toEqual(newDoc);
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
    expect(newDbDoc).toEqual(newDoc);
  });

  it("can delete a document", async () => {
    const row = {
      key: docId,
      id: docId,
      value: { op: "delete" as MigrationOps, data: {} },
    };
    const newDoc = await migrateOne({ row, db });
    expect(newDoc).toMatchObject({ _deleted: true });

    expect(db.get(docId)).rejects.toThrow();
  });

  it("can apply an update to a document", async () => {
    const row = {
      key: docId,
      id: docId,
      value: {
        op: "merge" as MigrationOps,
        data: { key: "new value" },
      },
    };
    const newDoc = await migrateOne({ row, db });
    expect(newDoc.data.key).toEqual(["value", "new value"]);

    const newDbDoc = await db.get(docId);
    expect(newDbDoc).toEqual(newDoc);
  });

  it("throws an error if the update operator is not recognized", async () => {
    const row = {
      key: docId,
      id: docId,
      value: {
        op: "not a real operator" as MigrationOps,
        data: { key: "new value" },
      },
    };
    await expect(migrateOne({ row, db })).rejects.toThrow(MigrationError);
  });
});
