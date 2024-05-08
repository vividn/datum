import { DataArgs } from "../dataArgs";
import { flexiblePositional } from "../flexiblePositional";

describe("flexiblePositional", () => {
  it("can transform special args into data type args with a corresponding key", () => {
    const args: DataArgs & { field: string; extraKey: string } = {
      field: "fieldArg",
      data: ["arg1", "arg2"],
      extraKey: "extraArg",
      keys: ["key1", "key2"],
    };

    flexiblePositional(args, "extraKey", "extraKey=default");
    expect(args).toEqual({
      field: "fieldArg",
      data: ["extraArg", "arg1", "arg2"],
      keys: ["extraKey=default", "key1", "key2"],
    });
    expect(args).not.toHaveProperty("extraArg");

    flexiblePositional(args, "field", "field");
    expect(args).toEqual({
      data: ["fieldArg", "extraArg", "arg1", "arg2"],
      keys: ["field", "extraKey=default", "key1", "key2"],
    });
    expect(args).not.toHaveProperty("field");
  });

  it("can skip adding the key and just move the value into data. useful for --fieldless, where the argparser still has the postional argument field, but it no longer corresponds to field, it's just normal data", () => {
    const args: DataArgs & { field: string } = {
      data: ["arg2", "arg3"],
      field: "arg1",
      keys: ["key1", "key2", "key3"],
    };
    flexiblePositional(args, "field", "field", true);
    expect(args).toEqual({
      data: ["arg1", "arg2", "arg3"],
      keys: ["key1", "key2", "key3"],
    });
  });

  it("does not change keys or data if the referenced positional key is undefined", () => {
    const args: DataArgs & { field?: string } = {
      data: ["arg1", "arg2"],
      keys: ["key1", "key2"],
    };
    flexiblePositional(args, "field", "field");
    expect(args).toEqual({
      data: ["arg1", "arg2"],
      keys: ["key1", "key2"],
    });

    const args2: DataArgs & { field?: string } = {};
    flexiblePositional(args2, "field", "field");
    expect(args2).not.toHaveProperty("data");
    expect(args2).not.toHaveProperty("keys");
  });
});
