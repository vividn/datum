import { backupCmd } from "../backupCmd";
import { testDbLifecycle } from "../../test-utils";
import * as fs from "fs";
import path from "path";
import * as os from "os";
import { addCmd } from "../addCmd";
import { occurCmd } from "../occurCmd";
import { EitherDocument } from "../../documentControl/DatumDocument";

describe("backupCmd", () => {
  const dbName = "backup_cmd_test";
  const db = testDbLifecycle(dbName);

  let backupFilePath: string;
  let dbDocs: EitherDocument[] = [];
  beforeEach(async () => {
    backupFilePath = path.join(os.tmpdir(), "backup.json.br");
    if (fs.existsSync(backupFilePath)) {
      fs.unlinkSync(backupFilePath);
    }
    dbDocs.push(
      await addCmd({
        field: "added_field",
        baseData: { some: "data", another: "field" },
      })
    );
    dbDocs.push(
      await occurCmd({
        field: "added_field",
        baseData: { some: "data", another: "field" },
      })
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

  it("does not clobber the backup file by default", async () => {
    await backupCmd({
      filename: backupFilePath,
    });
    expect(fs.existsSync(backupFilePath)).toBe(true);
    await expect(backupCmd({ filename: backupFilePath })).rejects.toThrow(
      "File exists"
    );
  });

  it("allows clobber if --clobber is specified", async () => {
    await backupCmd({
      filename: backupFilePath,
    });
    expect(fs.existsSync(backupFilePath)).toBe(true);
    await backupCmd({
      filename: backupFilePath,
      clobber: true,
    });
    expect(fs.existsSync(backupFilePath)).toBe(false);
  });
});
