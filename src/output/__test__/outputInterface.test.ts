import { ConsoleOutputProvider } from "../consoleOutput";
import { BrowserOutputProvider, BrowserOutputType } from "../browserOutput";
import { ExtractedAndFormatted } from "../outputInterface";
import { ACTIONS } from "../actions";

describe("OutputInterface implementations", () => {
  // Sample formatted data for testing
  const sampleFormatted: ExtractedAndFormatted = {
    action: "CREATE",
    id: "doc123",
    hid: "abc12",
    time: {
      hybrid: "10:00am",
      occur: "10:00am",
      modify: "m10:05am",
      create: "c10:00am",
      none: undefined
    },
    field: "test-field",
    state: "active",
    dur: "5m"
  };

  const sampleData = {
    field: "test-field",
    state: "active",
    occurTime: "2023-01-01T10:00:00Z"
  };

  const sampleMeta = {
    humanId: "abc12345",
    createTime: "2023-01-01T10:00:00Z",
    modifyTime: "2023-01-01T10:05:00Z"
  };

  describe("ConsoleOutputProvider", () => {
    let consoleOutput: ConsoleOutputProvider;
    let consoleLogSpy: jest.SpyInstance;
    
    beforeEach(() => {
      consoleOutput = new ConsoleOutputProvider();
      consoleLogSpy = jest.spyOn(console, "log").mockImplementation();
    });
    
    afterEach(() => {
      consoleLogSpy.mockRestore();
    });

    test("showHeaderLine should log header information", () => {
      consoleOutput.showHeaderLine(sampleFormatted);
      expect(consoleLogSpy).toHaveBeenCalledTimes(1);
      // The exact output will depend on chalk formatting
      expect(consoleLogSpy).toHaveBeenCalledWith(expect.any(String));
    });

    test("showMainInfoLine should log main information", () => {
      consoleOutput.showMainInfoLine(sampleFormatted);
      expect(consoleLogSpy).toHaveBeenCalledTimes(1);
      // The exact output will depend on chalk formatting
      expect(consoleLogSpy).toHaveBeenCalledWith(expect.any(String));
    });

    test("showCustomFormat should log formatted string", () => {
      consoleOutput.showCustomFormat(sampleData, sampleMeta, "Field: %field%, State: %state%");
      expect(consoleLogSpy).toHaveBeenCalledTimes(1);
      expect(consoleLogSpy).toHaveBeenCalledWith(expect.stringContaining("Field: test-field, State: active"));
    });
  });

  describe("BrowserOutputProvider", () => {
    let browserOutput: BrowserOutputProvider;
    
    beforeEach(() => {
      browserOutput = new BrowserOutputProvider();
    });

    test("showHeaderLine should add header to outputs", () => {
      browserOutput.showHeaderLine(sampleFormatted);
      const outputs = browserOutput.getOutputs();
      expect(outputs).toHaveLength(1);
      expect(outputs[0].type).toBe(BrowserOutputType.Header);
      expect(outputs[0].content).toEqual(expect.any(String));
    });

    test("showMainInfoLine should add info to outputs", () => {
      browserOutput.showMainInfoLine(sampleFormatted);
      const outputs = browserOutput.getOutputs();
      expect(outputs).toHaveLength(1);
      expect(outputs[0].type).toBe(BrowserOutputType.Info);
      expect(outputs[0].content).toEqual(expect.any(String));
    });

    test("showCustomFormat should add formatted content to outputs", () => {
      browserOutput.showCustomFormat(sampleData, sampleMeta, "Field: %field%, State: %state%");
      const outputs = browserOutput.getOutputs();
      expect(outputs).toHaveLength(1);
      expect(outputs[0].type).toBe(BrowserOutputType.CustomFormat);
      expect(outputs[0].content).toEqual(expect.stringContaining("Field: test-field, State: active"));
    });

    test("addSvgOutput should add SVG content", () => {
      const svgContent = "<svg>test</svg>";
      browserOutput.addSvgOutput(svgContent);
      const outputs = browserOutput.getOutputs();
      expect(outputs).toHaveLength(1);
      expect(outputs[0].type).toBe(BrowserOutputType.SVG);
      expect(outputs[0].content).toBe(svgContent);
    });

    test("clearOutputs should remove all outputs", () => {
      browserOutput.showHeaderLine(sampleFormatted);
      browserOutput.showMainInfoLine(sampleFormatted);
      expect(browserOutput.getOutputs()).toHaveLength(2);
      
      browserOutput.clearOutputs();
      expect(browserOutput.getOutputs()).toHaveLength(0);
    });

    test("addError should add error output", () => {
      const error = new Error("Test error");
      browserOutput.addError(error);
      const outputs = browserOutput.getOutputs();
      expect(outputs).toHaveLength(1);
      expect(outputs[0].type).toBe(BrowserOutputType.Error);
      expect(outputs[0].content).toBe("Test error");
    });

    test("showRename should add rename info to outputs", () => {
      browserOutput.showRename("oldId", "newId", ACTIONS.Rename);
      const outputs = browserOutput.getOutputs();
      expect(outputs).toHaveLength(1);
      expect(outputs[0].type).toBe(BrowserOutputType.Rename);
      expect(outputs[0].content).toEqual({
        action: ACTIONS.Rename,
        beforeId: "oldId",
        afterId: "newId"
      });
    });
  });
});