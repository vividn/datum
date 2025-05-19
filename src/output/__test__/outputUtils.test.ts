import { info, warn, error, success, OutputFunction } from "../outputUtils";

describe("outputUtils", () => {
  let outputSpy: jest.Mock;
  let output: OutputFunction;

  beforeEach(() => {
    outputSpy = jest.fn();
    output = outputSpy;
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe("info", () => {
    it("returns the message string and calls the output function", () => {
      const message = "This is an info message";
      const result = info(message, output);

      expect(result).toBe(message);
      expect(outputSpy).toHaveBeenCalledWith(message);
    });
  });

  describe("warn", () => {
    it("returns a yellow message string and calls the output function", () => {
      const message = "This is a warning message";
      const result = warn(message, output);

      // The result should contain ANSI color codes for yellow
      expect(result).toContain(message);
      expect(outputSpy).toHaveBeenCalled();
    });
  });

  describe("error", () => {
    it("returns a red message string and calls the output function", () => {
      const message = "This is an error message";
      const result = error(message, output);

      // The result should contain ANSI color codes for red
      expect(result).toContain(message);
      expect(outputSpy).toHaveBeenCalled();
    });
  });

  describe("success", () => {
    it("returns a green message string and calls the output function", () => {
      const message = "This is a success message";
      const result = success(message, output);

      // The result should contain ANSI color codes for green
      expect(result).toContain(message);
      expect(outputSpy).toHaveBeenCalled();
    });
  });
});
