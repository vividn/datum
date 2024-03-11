import * as connectDbModule from "../../auth/connectDb";
import { connectDb } from "../../auth/connectDb";
import { EitherDocument } from "../../documentControl/DatumDocument";
import { pass, resetTestDb } from "../../__test__/test-utils";

describe("tableOutput", () => {
  const docs: EitherDocument[] = [];

  beforeAll(async () => {
    // tmp db to generate docs easily
    const db = connectDb({ db: "test_table_output" });
    await resetTestDb(db);
    jest.spyOn(connectDbModule, "connectDb").mockReturnValue(db);

    
    await db.destroy().catch(pass);
  })
  it("Returns a table for an array of documents", () => {

  });
  it.todo("Can return a table with a different time metric");
  it.todo("Returns undefined if show is None");
  it.todo("Returns just the headers if documents is an empty array");
  it.todo("Can return with extra columns");
  it.todo("Can return using a format string");
});
