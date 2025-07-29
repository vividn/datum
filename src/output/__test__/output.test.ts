import {
  customFormat,
  showRename,
  showSingle,
  showCreate,
  showExists,
  showNoDiff,
  showFailed,
  showDelete,
  showUpdate,
  showOWrite,
} from "../output";
import {
  EitherDocument,
  EitherPayload,
} from "../../documentControl/DatumDocument";
import { OutputArgs, Show } from "../../input/outputArgs";
import { ACTIONS } from "../format";
import chalk from "chalk";

// Mock dependencies
jest.mock("../format", () => ({
  ...jest.requireActual("../format"),
  extractFormatted: jest.fn(),
  formattedDoc: jest.fn(),
  formattedNonRedundantData: jest.fn(),
  actionId: jest.fn(),
}));

jest.mock("../../utils/interpolateFields", () => ({
  interpolateFields: jest.fn(),
}));

jest.mock("../../utils/pullOutData", () => ({
  pullOutData: jest.fn(),
}));

import {
  extractFormatted,
  formattedDoc,
  formattedNonRedundantData,
  actionId,
} from "../format";
import { interpolateFields } from "../../utils/interpolateFields";
import { pullOutData } from "../../utils/pullOutData";

describe("output", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    chalk.level = 0; // Disable chalk colors for consistent testing
  });

  const mockDocument: EitherDocument = {
    _id: "test-id",
    _rev: "1-abc",
    field: "test-field",
    state: "test-state",
    occurTime: { utc: "2025-01-29T10:00:00Z" },
  };

  const mockPayload: EitherPayload = {
    field: "test-field",
    state: "test-state",
  };

  const mockExtracted = {
    action: "CREATE",
    id: "test-id",
    hid: "hum12",
    time: {
      occur: "10:00",
      none: undefined,
    },
    field: "test-field",
    state: "test-state",
    dur: "5m",
  };

  const mockOutputLineFn = jest.fn();

  const defaultOutputArgs: OutputArgs = {
    show: Show.Default,
    outputLineFn: mockOutputLineFn,
  };

  describe("customFormat", () => {
    it("interpolates fields from payload", () => {
      const formatString = "Field: %field%, State: %state%";
      const mockData = { field: "test", state: "active" };
      const mockMeta = { humanId: "hum12" };

      (pullOutData as jest.Mock).mockReturnValue({
        data: mockData,
        meta: mockMeta,
      });
      (interpolateFields as jest.Mock).mockReturnValue(
        "Field: test, State: active",
      );

      const result = customFormat(mockPayload, formatString);

      expect(pullOutData).toHaveBeenCalledWith(mockPayload);
      expect(interpolateFields).toHaveBeenCalledWith({
        data: mockData,
        meta: mockMeta,
        format: formatString,
      });
      expect(result).toBe("Field: test, State: active");
    });
  });

  describe("showRename", () => {
    it("returns undefined when show is None", () => {
      const result = showRename("old-id", "new-id", { show: Show.None });
      expect(result).toBeUndefined();
      expect(mockOutputLineFn).not.toHaveBeenCalled();
    });

    it("returns undefined when show is Format", () => {
      const result = showRename("old-id", "new-id", { show: Show.Format });
      expect(result).toBeUndefined();
      expect(mockOutputLineFn).not.toHaveBeenCalled();
    });

    it("displays rename action with arrow", () => {
      (actionId as jest.Mock).mockReturnValue("RENAME old-id");

      const result = showRename("old-id", "new-id", defaultOutputArgs);

      expect(actionId).toHaveBeenCalledWith(ACTIONS.Rename, "old-id");
      expect(result).toBe("RENAME old-id ⟶ new-id");
      expect(mockOutputLineFn).toHaveBeenCalledWith("RENAME old-id ⟶ new-id");
    });
  });

  describe("showSingle", () => {
    beforeEach(() => {
      (extractFormatted as jest.Mock).mockReturnValue(mockExtracted);
    });

    it("returns undefined when show is None", () => {
      const result = showSingle(ACTIONS.Create, mockDocument, {
        show: Show.None,
      });
      expect(result).toBeUndefined();
      expect(mockOutputLineFn).not.toHaveBeenCalled();
    });

    it("throws error when Format show requested without format string", () => {
      expect(() => {
        showSingle(ACTIONS.Create, mockDocument, { show: Show.Format });
      }).toThrow(
        "MissingArgument: formatted show requested without a format string",
      );
    });

    it("displays custom format when show is Format", () => {
      const formatString = "Custom: %field%";
      (interpolateFields as jest.Mock).mockReturnValue("Custom: test-field");
      (pullOutData as jest.Mock).mockReturnValue({
        data: mockDocument,
        meta: {},
      });

      const result = showSingle(ACTIONS.Create, mockDocument, {
        show: Show.Format,
        formatString,
        outputLineFn: mockOutputLineFn,
      });

      expect(result).toBe("Custom: test-field");
      expect(mockOutputLineFn).toHaveBeenCalledWith("Custom: test-field");
    });

    it("returns undefined for NoDiff action in Minimal mode", () => {
      const result = showSingle(ACTIONS.NoDiff, mockDocument, {
        show: Show.Minimal,
        outputLineFn: mockOutputLineFn,
      });

      expect(result).toBeUndefined();
      expect(mockOutputLineFn).not.toHaveBeenCalled();
    });

    it("displays only header line in Minimal mode", () => {
      const result = showSingle(ACTIONS.Create, mockDocument, {
        show: Show.Minimal,
        outputLineFn: mockOutputLineFn,
      });

      expect(result).toBe("CREATE hum12 test-id");
      expect(mockOutputLineFn).toHaveBeenCalledTimes(1);
      expect(mockOutputLineFn).toHaveBeenCalledWith("CREATE hum12 test-id");
    });

    it("displays header and main info in Default mode", () => {
      const result = showSingle(
        ACTIONS.Create,
        mockDocument,
        defaultOutputArgs,
      );

      expect(result).toBe(
        "CREATE hum12 test-id\n10:00 test-field test-state 5m",
      );
      expect(mockOutputLineFn).toHaveBeenCalledWith("CREATE hum12 test-id");
      expect(mockOutputLineFn).toHaveBeenCalledWith(
        "10:00 test-field test-state 5m",
      );
    });

    it("includes custom format when provided with Default show", () => {
      (interpolateFields as jest.Mock).mockReturnValue("Custom output");
      (pullOutData as jest.Mock).mockReturnValue({
        data: mockDocument,
        meta: {},
      });

      const result = showSingle(ACTIONS.Create, mockDocument, {
        show: Show.Default,
        formatString: "Custom: %field%",
        outputLineFn: mockOutputLineFn,
      });

      expect(result).toContain("Custom output");
      expect(mockOutputLineFn).toHaveBeenCalledWith("Custom output");
    });

    it("displays full document in All mode", () => {
      (formattedDoc as jest.Mock).mockReturnValue(
        "{\n  _id: 'test-id',\n  field: 'test-field'\n}",
      );

      const result = showSingle(ACTIONS.Create, mockDocument, {
        show: Show.All,
        outputLineFn: mockOutputLineFn,
      });

      expect(formattedDoc).toHaveBeenCalledWith(mockDocument);
      expect(result).toContain(
        "{\n  _id: 'test-id',\n  field: 'test-field'\n}",
      );
    });

    it("displays non-redundant data in Standard mode", () => {
      (formattedNonRedundantData as jest.Mock).mockReturnValue(
        "{ extra: 'data' }",
      );

      const result = showSingle(ACTIONS.Create, mockDocument, {
        show: Show.Standard,
        outputLineFn: mockOutputLineFn,
      });

      expect(formattedNonRedundantData).toHaveBeenCalledWith(mockDocument);
      expect(result).toContain("{ extra: 'data' }");
    });

    it("skips non-redundant data when undefined", () => {
      (formattedNonRedundantData as jest.Mock).mockReturnValue(undefined);

      const result = showSingle(ACTIONS.Create, mockDocument, {
        show: Show.Standard,
        outputLineFn: mockOutputLineFn,
      });

      expect(result).not.toContain("undefined");
    });

    it("handles missing main info line", () => {
      (extractFormatted as jest.Mock).mockReturnValue({
        ...mockExtracted,
        time: { none: undefined },
        field: undefined,
        state: undefined,
        dur: undefined,
      });

      const result = showSingle(
        ACTIONS.Create,
        mockDocument,
        defaultOutputArgs,
      );

      expect(result).toBe("CREATE hum12 test-id");
      expect(mockOutputLineFn).toHaveBeenCalledTimes(1);
    });
  });

  describe("action-specific show functions", () => {
    beforeEach(() => {
      (extractFormatted as jest.Mock).mockReturnValue(mockExtracted);
    });

    it("showCreate calls showSingle with Create action", () => {
      showCreate(mockDocument, defaultOutputArgs);
      expect(extractFormatted).toHaveBeenCalledWith(
        mockDocument,
        ACTIONS.Create,
      );
    });

    it("showExists calls showSingle with Exists action", () => {
      showExists(mockDocument, defaultOutputArgs);
      expect(extractFormatted).toHaveBeenCalledWith(
        mockDocument,
        ACTIONS.Exists,
      );
    });

    it("showNoDiff calls showSingle with NoDiff action", () => {
      showNoDiff(mockDocument, defaultOutputArgs);
      expect(extractFormatted).toHaveBeenCalledWith(
        mockDocument,
        ACTIONS.NoDiff,
      );
    });

    it("showFailed converts payload to document", () => {
      showFailed(mockPayload, defaultOutputArgs);
      expect(extractFormatted).toHaveBeenCalledWith(
        expect.objectContaining({
          _id: "",
          _rev: "",
          ...mockPayload,
        }),
        ACTIONS.Failed,
      );
    });

    it("showDelete converts payload to document", () => {
      showDelete(mockPayload, defaultOutputArgs);
      expect(extractFormatted).toHaveBeenCalledWith(
        expect.objectContaining({
          _id: "",
          _rev: "",
          ...mockPayload,
        }),
        ACTIONS.Delete,
      );
    });

    it("showUpdate uses afterDoc", () => {
      const beforeDoc = { ...mockDocument, state: "old-state" };
      const afterDoc = { ...mockDocument, state: "new-state" };

      showUpdate(beforeDoc, afterDoc, defaultOutputArgs);
      expect(extractFormatted).toHaveBeenCalledWith(afterDoc, ACTIONS.Update);
    });

    it("showOWrite uses afterDoc", () => {
      const beforeDoc = { ...mockDocument, state: "old-state" };
      const afterDoc = { ...mockDocument, state: "new-state" };

      showOWrite(beforeDoc, afterDoc, defaultOutputArgs);
      expect(extractFormatted).toHaveBeenCalledWith(afterDoc, ACTIONS.OWrite);
    });
  });

  describe("sanitizeOutputArgs", () => {
    beforeEach(() => {
      (extractFormatted as jest.Mock).mockReturnValue(mockExtracted);
    });

    it("defaults to console.log when outputLineFn not provided", () => {
      const consoleLogSpy = jest.spyOn(console, "log").mockImplementation();

      showSingle(ACTIONS.Create, mockDocument, {
        show: Show.Minimal,
      });

      expect(consoleLogSpy).toHaveBeenCalled();
      consoleLogSpy.mockRestore();
    });

    it("prioritizes showAll over show parameter", () => {
      (formattedDoc as jest.Mock).mockReturnValue("full doc");

      showSingle(ACTIONS.Create, mockDocument, {
        show: Show.Minimal,
        showAll: true,
        outputLineFn: mockOutputLineFn,
      });

      expect(formattedDoc).toHaveBeenCalled();
    });

    it("defaults to Format when formatString provided without show", () => {
      (interpolateFields as jest.Mock).mockReturnValue("formatted");
      (pullOutData as jest.Mock).mockReturnValue({ data: {}, meta: {} });

      showSingle(ACTIONS.Create, mockDocument, {
        formatString: "%field%",
        outputLineFn: mockOutputLineFn,
      });

      expect(interpolateFields).toHaveBeenCalled();
    });

    it("defaults to None when no show parameters provided", () => {
      const result = showSingle(ACTIONS.Create, mockDocument, {
        outputLineFn: mockOutputLineFn,
      });

      expect(result).toBeUndefined();
      expect(mockOutputLineFn).not.toHaveBeenCalled();
    });
  });
});
