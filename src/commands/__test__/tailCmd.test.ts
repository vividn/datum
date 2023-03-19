import { mockedLogLifecycle, pass, resetTestDb } from "../../test-utils";
import { EitherPayload } from "../../documentControl/DatumDocument";

describe("tailCmd", () => {
  const _mockedLog = mockedLogLifecycle();
  const dbName = "tail_cmd_test";
  let db: PouchDB.Database<EitherPayload>;

  beforeEach(async () => {
    db = await resetTestDb(dbName);
  });

  afterEach(async () => {
    await db.destroy().catch(pass);
  });

  it.todo("displays the last 10 occurences in the database");
  it.todo("can display the last n occurences");
  it.todo("can display a custom format for the tail commands");
});
