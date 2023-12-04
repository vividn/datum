import { backupCmd } from "../backupCmd";
import { at, testDbLifecycle } from "../../test-utils";
import * as fs from "fs";
import path from "path";
import * as os from "os";
import { addCmd } from "../addCmd";
import { occurCmd } from "../occurCmd";
import { EitherDocument } from "../../documentControl/DatumDocument";

describe("backupCmd", () => {
  const dbName = "backup_cmd_test";
  const _db = testDbLifecycle(dbName);

  let backupFilePath: string;
  let dbDocs: EitherDocument[] = [];
  beforeEach(async () => {
    const tempDir = process.env["RUNNER_TEMP"] ?? os.tmpdir();
    backupFilePath = path.join(tempDir, "backup.json");
    if (fs.existsSync(backupFilePath)) {
      fs.unlinkSync(backupFilePath);
    }
    dbDocs.push(
      await addCmd({
        field: "added_field",
        baseData: { some: "data", another: "field" },
      }),
    );
    dbDocs.push(
      await occurCmd({
        field: "occurred_field",
        baseData: { some: "data", another: "field" },
      }),
    );
  });

  afterEach(() => {
    if (fs.existsSync(backupFilePath)) {
      fs.unlinkSync(backupFilePath);
    }
    dbDocs = [];
  });

  it("creates a backup file that can contains all the documents", async () => {
    expect(fs.existsSync(backupFilePath)).toBe(false);
    await backupCmd({
      filename: backupFilePath,
    });
    expect(fs.existsSync(backupFilePath)).toBe(true);
    const loadedBackup = JSON.parse(fs.readFileSync(backupFilePath).toString());
    const docs = loadedBackup.docs;
    expect(docs.length).toBe(dbDocs.length);
    dbDocs.forEach((doc) => {
      expect(docs).toContainEqual(doc);
    });
  });

  it("does not overwrite the backup file by default", async () => {
    await backupCmd({
      filename: backupFilePath,
    });
    expect(fs.existsSync(backupFilePath)).toBe(true);
    const newDoc = await occurCmd({ field: "occuredField2" });
    await expect(backupCmd({ filename: backupFilePath })).rejects.toThrow(
      "File exists",
    );

    expect(fs.existsSync(backupFilePath)).toBe(true);
    const loadedBackup = JSON.parse(fs.readFileSync(backupFilePath).toString());
    const docs = loadedBackup.docs;
    expect(docs.length).toBe(dbDocs.length);
    expect(docs).not.toContainEqual(newDoc);
  });

  it("allows overwrite if --overwrite is specified", async () => {
    await backupCmd({
      filename: backupFilePath,
    });
    expect(fs.existsSync(backupFilePath)).toBe(true);
    const newDoc = await occurCmd({ field: "occuredField2" });
    await backupCmd({
      filename: backupFilePath,
      overwrite: true,
    });

    expect(fs.existsSync(backupFilePath)).toBe(true);
    const loadedBackup = JSON.parse(fs.readFileSync(backupFilePath).toString());
    const docs = loadedBackup.docs;
    expect(docs.length).toBe(dbDocs.length + 1);
    expect(docs).toContainEqual(newDoc);
  });

  it("stores the backup time in the backup file json, near the beginning of the file", async () => {
    const backupTime = "2023-11-17T17:22:00.000Z";
    await at(
      backupTime,
      backupCmd,
    )({
      filename: backupFilePath,
    });
    expect(fs.existsSync(backupFilePath)).toBe(true);
    const rawBackupFile = fs.readFileSync(backupFilePath).toString();
    const loadedBackup = JSON.parse(rawBackupFile);
    expect(loadedBackup.backupTime).toEqual(backupTime);
    const firstBytes = rawBackupFile.slice(0, 50);
    expect(firstBytes).toContain(backupTime);
  });
});
