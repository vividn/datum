import { DataArgs } from "../dataArgs";
import { flexiblePositional } from "../flexiblePositional";

describe("flexiblePositional", () => {
  it("can move different arguments into optional and required keys", () => {
    const args: DataArgs & { field: string; extraArg: string } = {
      field: "arg0",
      data: ["arg2", "arg3"],
      extraArg: "arg1",
      required: ["req1", "req2"],
      optional: ["opt1", "opt2"],
    };

    flexiblePositional(args, "extraArg", "optional");
    expect(args).toEqual({
      field: "arg0",
      data: ["arg1", "arg2", "arg3"],
      required: ["req1", "req2"],
      optional: ["extraArg", "opt1", "opt2"],
    });
    expect(args).not.toHaveProperty("extraArg");

    flexiblePositional(args, "field", "required");
    expect(args).toEqual({
      data: ["arg0", "arg1", "arg2", "arg3"],
      required: ["field", "req1", "req2"],
      optional: ["extraArg", "opt1", "opt2"],
    });
    expect(args).not.toHaveProperty("field");
  });

  it("can move arguments into a data key with a custom name", () => {
    const args: DataArgs & { extraArg: string | number } = {
      data: ["arg2", "arg3"],
      extraArg: "arg1",
      required: ["req1", "req2"],
      optional: ["opt1"],
    };

    flexiblePositional(args, "extraArg", "optional", "__extraArg");
    expect(args).toEqual({
      data: ["arg1", "arg2", "arg3"],
      required: ["req1", "req2"],
      optional: ["__extraArg", "opt1"],
    });
  });

  it("can just move the value into data without adding the key to required or optional", () => {
    const args: DataArgs & { extraArg: string | number } = {
      data: ["arg2", "arg3"],
      extraArg: "arg1",
      required: ["req1", "req2"],
      optional: ["opt1"],
    };
    flexiblePositional(args, "extraArg", false);
    expect(args).toEqual({
      data: ["arg1", "arg2", "arg3"],
      required: ["req1", "req2"],
      optional: "opt1",
    });
  });

  it("does not change required, optional, or data, if the referenced positional key is undefined", () => {
    const args: DataArgs & { field?: string } = {
      data: ["arg2", "arg3"],
      required: ["req1", "req2"],
      optional: ["opt1", "opt2"],
    };
    flexiblePositional(args, "field", "required");
    expect(args).toEqual({
      data: ["arg2", "arg3"],
      required: ["req1", "req2"],
      optional: ["opt1", "opt2"],
    });

    const args2: DataArgs & { field?: string } = {
      required: ["req1"],
    };
    flexiblePositional(args2, "field", "optional");
    expect(args2).not.toHaveProperty("data");
    expect(args2).not.toHaveProperty("optional");
  });
});
