import { testDbLifecycle, mockedLogLifecycle } from "../../__test__/test-utils";
import { setupCmd } from "../setupCmd";
import { specCmd } from "../specCmd";
import { resetSpecCache } from "../../field/mySpecs";
import { specDocId } from "../../field/specDb";

jest.mock("prompts", () => ({
  __esModule: true,
  default: jest.fn(),
}));

describe("specCmd", () => {
  const dbName = "spec_cmd_test";
  const db = testDbLifecycle(dbName);
  const { mockedLog } = mockedLogLifecycle();

  beforeEach(async () => {
    resetSpecCache();
    await setupCmd("");
  });

  it("should display help if no field provided", async () => {
    await specCmd([], { db: dbName });
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("usage"));
  });

  it("should show field spec when it exists", async () => {
    await specCmd(["field", "sleep"], { db: dbName });
    
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("Spec for field: sleep"));
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("Kind: start"));
  });

  it("should update field spec with command line args", async () => {
    await specCmd(["sleep", "--color", "#ff0000"], { db: dbName });
    
    // Verify the document was added to the database
    const doc = await db.get(specDocId("sleep"));
    expect(doc.data).toEqual(expect.objectContaining({
      color: "#ff0000"
    }));
    
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("Updated spec for field: sleep"));
  });

  it("should handle migration", async () => {
    await specCmd(["--migrate"], { db: dbName });
    
    // Verify migration completed message
    expect(mockedLog).toHaveBeenCalledWith(expect.stringContaining("Migration complete"));
    
    // Verify at least some specs were migrated
    const result = await db.allDocs({
      include_docs: true,
      startkey: 'SPEC:',
      endkey: 'SPEC:\ufff0',
    });
    
    expect(result.rows.length).toBeGreaterThan(0);
  });
});