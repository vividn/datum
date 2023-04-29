import { testDbLifecycle } from "../../test-utils";
import { insertDatumView } from "../../views/insertDatumView";
import { keyValueView } from "../../views/datumViews";

describe("mapCmd", () => {
  const dbName = "map_cmd_test";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    await insertDatumView({ db, datumView: keyValueView });
    await db.bulkDocs([
      { _id: "1", value: "one", key: "1" },
      { _id: "2", value: "two", key: "2" },
      { _id: "3", value: "three", key: "3" },
      { _id: "4", value: "four", key: "4" },
      { _id: "5", value: "five", key: "5" },
      { _id: "other", vey: "other", kalue: "other" },
    ]);
  });

  it.todo("displays all the rows of the default map function");
  it.todo(
    "still displays the map table even if a reduce function is specified"
  );
  it.todo("can specify a different map function using /notation");
  it.todo(
    "prioritizes a design document with / in it over using the named function of whats prior to the slash"
  );
  it.todo("can display a different map function by specifying view");
  it.todo("can easily specify the startsWith");
  it.todo(
    "uses startkey and endkey if another value is passed after startsWith"
  );

  it.todo("lists available maps and reduces if no arguments are given");

  it.todo("can list the _all_docs view from the database");
  it.todo("does not render if Show is None");
  it.todo("it does render if Show is not none");
});
