import { restoreCmd } from "../restoreCmd";
import { testDbLifecycle } from "../../test-utils";
import path from "path";
import * as os from "os";
import * as fs from "fs";
import { generateSampleMorning } from "./tailCmd.test";
import { setupCmd } from "../setupCmd";
import { backupCmd } from "../backupCmd";
import * as connectDbModule from "../../auth/connectDb";
import { connectDb } from "../../auth/connectDb";
import { addCmd } from "../addCmd";

describe("restoreCmd", () => {
  const dbName1 = "restore_cmd_test_1";
  const dbName2 = "restore_cmd_test_2";
  const db2 = testDbLifecycle(dbName2);

  // this one takes precedence for the default db
  const db1 = testDbLifecycle(dbName1);

  let backupFilePath: string;
  beforeEach(async () => {
    const tempDir = process.env["RUNNER_TEMP"] ?? os.tmpdir();
    backupFilePath = path.join(tempDir, "backup.json");
    if (fs.existsSync(backupFilePath)) {
      fs.unlinkSync(backupFilePath);
    }
    await setupCmd({});
    await generateSampleMorning("2023-11-17");
    expect((await db1.info()).doc_count).toBeGreaterThan(5);
    expect((await db2.info()).doc_count).toBe(0);

    await backupCmd({ filename: backupFilePath });
  });
  afterEach(() => {
    if (fs.existsSync(backupFilePath)) {
      fs.unlinkSync(backupFilePath);
    }
  });

  it("can restore a previously backed up db to a new empty db, having exactly the same docs and revisions", async () => {
    expect((await db2.info()).doc_count).toBe(0);

    jest.spyOn(connectDbModule, "connectDb").mockReturnValueOnce(db2);
    await restoreCmd({
      filename: backupFilePath,
    });
    expect((await db2.info()).doc_count).not.toBe(0);

    const restoredDocs = (await db2.allDocs({ include_docs: true })).rows.map(
      ({ doc }) => doc,
    );
    const originalDocs = (await db1.allDocs({ include_docs: true })).rows.map(
      ({ doc }) => doc,
    );
    expect(restoredDocs.length).toBe(originalDocs.length);
    expect(restoredDocs).toEqual(originalDocs);
  });

  it("creates a new db if it does not exist", async () => {
    const unmadeDbName = "restore_cmd_test_unmade_db";
    const spy = jest.spyOn(connectDbModule, "connectDb");
    spy.mockRestore();
    const unmadeDb = connectDb({ db: unmadeDbName, createDb: false });
    await restoreCmd({
      db: unmadeDbName,
      filename: backupFilePath,
    });
    expect((await unmadeDb.info()).doc_count).not.toBe(0);
    await unmadeDb.destroy();
  });

  it("throws an error if the db is not empty", async () => {
    expect((await db2.info()).doc_count).toBe(0);
    await db2.put({ _id: "one_document" });

    jest.spyOn(connectDbModule, "connectDb").mockReturnValueOnce(db2);
    await expect(
      restoreCmd({
        filename: backupFilePath,
      }),
    ).rejects.toThrowErrorMatchingSnapshot();
  });

  it("can restore over a nonempty db with the appropriate option", async () => {
    expect((await db2.info()).doc_count).toBe(0);
    jest.spyOn(connectDbModule, "connectDb").mockReturnValue(db2);
    const extraDoc = await addCmd({ field: "extra_doc" });

    await restoreCmd({
      filename: backupFilePath,
      allowNonempty: true,
    });

    const restoredDocs = (await db2.allDocs({ include_docs: true })).rows.map(
      ({ doc }) => doc,
    );
    const originalDocs = (await db1.allDocs({ include_docs: true })).rows.map(
      ({ doc }) => doc,
    );
    expect(restoredDocs.length).toBe(originalDocs.length + 1);
    expect(restoredDocs).toContainEqual(extraDoc);
    expect(restoredDocs).toContainEqual(originalDocs.at(1));
  });
});
