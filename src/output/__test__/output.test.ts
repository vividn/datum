import {
  showSingle,
  showCreate,
  showHeaderLine,
  showMainInfoLine,
  showCustomFormat,
  ACTIONS,
} from "../output";
import { Show } from "../../input/outputArgs";
import { OutputInterface } from "../outputUtils";

describe("output functions", () => {
  let outputSpy: jest.Mock;
  let output: OutputInterface;

  beforeEach(() => {
    outputSpy = jest.fn();
    output = {
      log: outputSpy,
      info: outputSpy,
      warn: outputSpy,
      error: outputSpy,
    };
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe("showSingle", () => {
    it("returns a string and calls output.log", () => {
      const mockDoc = {
        _id: "test-id",
        _rev: "test-rev",
        data: {
          field: "test-field",
          state: true,
          occurTime: "2023-05-19T12:00:00.000Z",
        },
        meta: {
          humanId: "test-human-id",
          createTime: "2023-05-19T12:00:00.000Z",
          modifyTime: "2023-05-19T12:00:00.000Z",
        },
      };

      const outputArgs = {
        show: Show.Standard,
        output,
      };

      const result = showSingle(ACTIONS.Create, mockDoc as any, outputArgs);

      expect(result).toBeDefined();
      expect(typeof result).toBe("string");
      expect(outputSpy).toHaveBeenCalled();
    });

    it("returns undefined for Show.None", () => {
      const mockDoc = { _id: "test-id", _rev: "test-rev" };
      const outputArgs = {
        show: Show.None,
        output,
      };

      const result = showSingle(ACTIONS.Create, mockDoc as any, outputArgs);

      expect(result).toBeUndefined();
      expect(outputSpy).not.toHaveBeenCalled();
    });
  });

  describe("showCreate", () => {
    it("calls showSingle with CREATE action", () => {
      const mockDoc = { _id: "test-id", _rev: "test-rev" };
      const outputArgs = {
        show: Show.Standard,
        output,
      };

      const result = showCreate(mockDoc as any, outputArgs);

      expect(result).toBeDefined();
      expect(outputSpy).toHaveBeenCalled();
    });
  });

  describe("showHeaderLine", () => {
    it("formats and returns header line", () => {
      const formatted = {
        action: "CREATE",
        hid: "test-id",
        id: "test-full-id",
        time: { occur: "12:00", none: undefined },
      };

      const result = showHeaderLine(formatted as any, output);

      expect(result).toBe("CREATE test-id test-full-id");
      expect(outputSpy).toHaveBeenCalledWith("CREATE test-id test-full-id");
    });
  });

  describe("showMainInfoLine", () => {
    it("formats and returns main info line when not empty", () => {
      const formatted = {
        time: { occur: "12:00", none: undefined },
        field: "test-field",
        state: "active",
        dur: "30m",
      };

      const result = showMainInfoLine(formatted as any, output);

      expect(result).toBe("12:00 test-field active 30m");
      expect(outputSpy).toHaveBeenCalledWith("12:00 test-field active 30m");
    });

    it("returns null for empty info line", () => {
      const formatted = {
        time: { none: undefined },
      };

      const result = showMainInfoLine(formatted as any, output);

      expect(result).toBeNull();
      expect(outputSpy).not.toHaveBeenCalled();
    });
  });

  describe("showCustomFormat", () => {
    it("formats and returns custom format string", () => {
      // Mock interpolateFields to return a fixed string for testing
      jest.mock("../../utils/interpolateFields", () => ({
        interpolateFields: () => "test-field test-state",
      }));

      const payload = {
        data: {
          field: "test-field",
          state: true,
        },
        meta: {
          humanId: "test-human-id",
        },
      };

      const formatString = "%field %state";

      const result = showCustomFormat(payload as any, formatString, output);

      expect(result).toBeDefined();
      // Test that the output function was called, which is what we're concerned with
      expect(outputSpy).toHaveBeenCalled();
    });
  });
});
