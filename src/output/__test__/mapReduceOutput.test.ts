import { mapReduceOutput } from "../mapReduceOutput";
import { OutputFunction } from "../outputUtils";

describe("mapReduceOutput", () => {
  let outputSpy: jest.Mock;
  let output: OutputFunction;

  beforeEach(() => {
    outputSpy = jest.fn();
    output = outputSpy;
    jest.spyOn(console, "log").mockImplementation(() => {});
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  it("formats map/reduce results as a table and calls output function", () => {
    const mockViewResponse = {
      rows: [
        {
          id: "doc1",
          key: "key1",
          value: "value1",
          doc: {
            _id: "doc1",
            _rev: "rev1",
            data: {
              field: "test-field",
              customColumn: "custom-value",
            },
            meta: {
              humanId: "human-id-1",
            },
          },
        },
        {
          id: "doc2",
          key: "key2",
          value: "value2",
          doc: {
            _id: "doc2",
            _rev: "rev2",
            data: {
              field: "test-field-2",
              customColumn: "custom-value-2",
            },
            meta: {
              humanId: "human-id-2",
            },
          },
        },
      ],
    };

    const result = mapReduceOutput(
      mockViewResponse as any,
      true,
      true,
      ["customColumn"],
      output,
    );

    expect(result).toHaveProperty("output");
    expect(result).toHaveProperty("rows");
    expect(outputSpy).toHaveBeenCalled();
  });

  it("handles responses without doc property", () => {
    const mockViewResponse = {
      rows: [
        {
          id: "doc1",
          key: "key1",
          value: "value1",
        },
        {
          id: "doc2",
          key: "key2",
          value: "value2",
        },
      ],
    };

    const result = mapReduceOutput(
      mockViewResponse as any,
      true,
      false,
      undefined,
      output,
    );

    expect(result).toHaveProperty("output");
    expect(result.rows.length).toBe(2);
    expect(outputSpy).toHaveBeenCalled();
  });

  it("handles array input format", () => {
    const mockRows = [
      {
        id: "doc1",
        key: "key1",
        value: "value1",
      },
      {
        id: "doc2",
        key: "key2",
        value: "value2",
      },
    ];

    const result = mapReduceOutput(
      mockRows as any,
      false,
      false,
      undefined,
      output,
    );

    expect(result).toHaveProperty("output");
    expect(result.rows.length).toBe(2);
    expect(outputSpy).toHaveBeenCalled();
  });
});
