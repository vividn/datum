import { OutputInterface, createStringOutput } from "../outputUtils";

describe("outputUtils", () => {
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

  describe("OutputInterface", () => {
    it("provides methods for outputting messages", () => {
      const message = "This is a test message";
      output.log(message);
      expect(outputSpy).toHaveBeenCalledWith(message);
    });
  });

  describe("createStringOutput", () => {
    it("creates an output interface that collects messages", () => {
      const stringOutput = createStringOutput();
      const message = "This is a message";
      stringOutput.log(message);
      expect(stringOutput.messages).toContain(message);
    });

    it("formats warning messages with yellow color", () => {
      const stringOutput = createStringOutput();
      const message = "This is a warning";
      stringOutput.warn(message);
      expect(stringOutput.messages[0]).toContain(message);
      // Should contain ANSI color codes
      expect(stringOutput.messages[0]).not.toBe(message);
    });

    it("formats error messages with red color", () => {
      const stringOutput = createStringOutput();
      const message = "This is an error";
      stringOutput.error(message);
      expect(stringOutput.messages[0]).toContain(message);
      // Should contain ANSI color codes
      expect(stringOutput.messages[0]).not.toBe(message);
    });
  });
});
