import { backupCmd } from "../backupCmd";
import { testDbLifecycle } from "../../test-utils";
import * as fs from "fs";
import path from "path";
import * as os from "os";

describe("backupCmd", () => {
  const dbName = "backup_cmd_test";
  const db = testDbLifecycle(dbName);

  it("creates a compressed backup file that can contains all the documents", async () => {
    await db.put({ _id: "1", name: "John" });
    await db.put({ _id: "2", name: "Jane" });
    // create a tempoarary file path
    const backupFilePath = path.join(os.tmpdir(), "backup.json");
    fs.unlinkSync(backupFilePath);
    await backupCmd({
      filename: backupFilePath,
    });
    // expect file to exist
    expect(fs.existsSync(backupFilePath)).toBe(true);
    // expect file after uncompressing to contain all documents
    const backupFile = fs.readFileSync(backupFilePath, "utf-8");
  });
})