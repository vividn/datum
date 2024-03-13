describe("mapCmd", () => {
  // const dbName = "map_cmd_test";
  // const db = testDbLifecycle(dbName);
  // const mockedLog = mockedLogLifecycle();

  it.todo("displays all the rows of the default map function");
  it.todo(
    "still displays the map table even if a reduce function is specified",
  );
  it.todo("can specify a different map function using /notation");
  it.todo(
    "prioritizes a design document with / in it over using the named function of whats prior to the slash",
  );
  it.todo("can display a different map function by specifying view");
  it.todo("can easily specify the startsWith");
  it.todo(
    "uses startkey and endkey if another value is passed after startsWith",
  );

  it.todo("lists available maps and reduces if no arguments are given");

  it.todo("can list the _all_docs view from the database");
  it.todo("can show the ids");
  it.todo("can show the humanIds");
  it.todo("does not render if Show is None");
  it.todo("it does render if Show is not none");
  it("renders extra columns", async () => {
    // Test case logic here
  });
  describe("reverse", () => {
    it.todo("can reverse when start and end are specified");
    it.todo("can reverse when just start is specified");
    it.todo("can reverse a already descending order");
    it.todo("can reverse the _all map");
  });
});
