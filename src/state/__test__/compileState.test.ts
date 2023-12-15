describe("compileState", () => {
  it("does nothing if there is no state and no occurTime", async () => {});

  it.todo(
    "leaves lastState as it is in the payload even if the real lastState is different",
  );
  it.todo(
    "adds lastState to the payload if is not there and there is state and occurTime",
  );
  it.todo(
    "adds lastState to a payload with occurTime and no state only if lastState is non false",
  );
  it.todo(
    "gives a warning if trying to add lastState from context and the db is not setup. After it, returns without lastState",
  );
});
