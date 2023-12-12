describe("compileState", () => {
  it("does nothing if there is no state and no occurTime");
  it("turns a state with just id into just the id");
  it("turns a complex state with id=null into just null, and warns");
  it("turns a complex state with id=false into just false, and warns");
  it("turns a complex state with id=true into the complex state without id");
  it("leaves a complex state with other id, or no id as it is");
  it("adds lastState to the payload if there is state and occurTime");
  it(
    "adds lastState to a payload with occurTime and no state only if lastState is non false",
  );
  it("does not overwrite lastState if it exists already in the payload");
  it("gives a warning if trying to add lastState and the db is not setup, and then returns without lastState");
});
